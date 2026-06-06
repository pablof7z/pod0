package io.f7z.podcast.capabilities

import android.content.Context
import android.os.Environment
import android.util.Log
import io.f7z.podcast.DownloadItemSnapshot
import io.f7z.podcast.KernelBridge
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import okhttp3.Call
import okhttp3.OkHttpClient
import okhttp3.Request
import java.io.File
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import kotlin.coroutines.coroutineContext

/**
 * `nmp.download.capability` executor for Android — the OkHttp-backed
 * counterpart to `App/Sources/Capabilities/DownloadCapability.swift`.
 *
 * ## Why a *pull* model (vs. iOS's push model)
 *
 * On iOS the kernel pushes a `DownloadCommand` to the capability through
 * `dispatch_capability`. Android now has the generic NMP callback for HTTP
 * and audio, but downloads intentionally keep a pull-model executor: the
 * capability reconciles against the kernel's projected `downloads.active`
 * rows, and the router acknowledges download commands without starting a
 * second path. The kernel still owns all policy (which episodes,
 * `max_concurrent`, wifi-only gate, retry); this class only mirrors that
 * intent into real HTTP fetches. (D7 — report/execute, never decide.)
 *
 * ## Single-writer reconcile (no double-start race)
 *
 * [`reconcile`] is the **sole** starter and canceller of downloads. It runs
 * on every snapshot tick:
 *
 *  * For each row with `state == "active"` and a non-blank `url` that is not
 *    already in flight → launch a download coroutine. Filtering on
 *    `"active"` (not `"queued"`) respects the kernel's concurrency cap; the
 *    kernel promotes a queued item to `active` only when a slot frees, and
 *    the next tick's reconcile picks it up.
 *  * For each in-flight episode that is **no longer present** as an active
 *    row → cancel its coroutine. A user-issued cancel resolves the kernel
 *    item to a terminal state, which drops it out of the snapshot; the
 *    disappearance is the only cancel signal Android acts on; pushed download
 *    commands are acknowledged by the generic router to avoid duplicate starts.
 *
 * The follow-up `DownloadCommand` returned by [`KernelBridge.downloadReport`]
 * is intentionally **ignored for starting**; the next snapshot tick is the
 * only starter.
 *
 * ## Lifecycle / UAF safety
 *
 * Downloads are scoped to [`scope`] (a `SupervisorJob`). [`detach`] marks the
 * capability detached before cancelling jobs and OkHttp calls, so no report
 * can fire through the bridge after `bridge.free()`. Reports are only
 * meaningful while the kernel is alive, which is also why this uses
 * foreground-scoped coroutines rather than WorkManager (a background-completed
 * report would have no live kernel to land on).
 */
class DownloadCapability(
    private val bridge: KernelBridge,
    private val context: Context,
) {
    private val client: OkHttpClient = OkHttpClient.Builder()
        .connectTimeout(30, TimeUnit.SECONDS)
        .readTimeout(60, TimeUnit.SECONDS)
        .retryOnConnectionFailure(true)
        .build()

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val detached = AtomicBoolean(false)

    /** episodeId → running download job. Touched from the snapshot coroutine
     *  (add, via reconcile) and the IO workers (remove, on finish). */
    private val inFlight = ConcurrentHashMap<String, Job>()
    private val inFlightCalls = ConcurrentHashMap<String, Call>()

    /**
     * Diff the kernel's active-download rows against what we're executing,
     * starting new fetches and cancelling vanished ones. Idempotent; safe to
     * call on every snapshot tick. Pass `null` (no downloads section) to mean
     * "no active downloads" — any in-flight job is then cancelled.
     */
    fun reconcile(active: List<DownloadItemSnapshot>?) {
        if (detached.get()) return
        val activeRows = active.orEmpty().filter { it.state == STATE_ACTIVE && it.url.isNotBlank() }
        val activeIds = activeRows.mapTo(HashSet()) { it.episodeId }

        // Cancel jobs whose episode is no longer an active row (user cancel,
        // kernel terminal transition). The disappearance is the cancel signal.
        for (episodeId in inFlight.keys.toList()) {
            if (episodeId !in activeIds) {
                cancelInFlight(episodeId)
            }
        }

        // Start fetches for active rows we are not already running.
        for (row in activeRows) {
            if (inFlight.containsKey(row.episodeId)) continue
            startDownload(row.episodeId, row.url, row.totalBytes)
        }
    }

    /** Cancel every in-flight download before the owner frees the bridge. */
    fun detach() {
        if (!detached.compareAndSet(false, true)) return
        inFlight.keys.toList().forEach(::cancelInFlight)
        inFlightCalls.values.forEach { call -> call.cancel() }
        scope.cancel()
    }

    private fun cancelInFlight(episodeId: String) {
        inFlightCalls.remove(episodeId)?.cancel()
        inFlight.remove(episodeId)?.cancel()
    }

    private fun startDownload(episodeId: String, url: String, hintBytes: Long?) {
        val dest = destinationFile(episodeId, url)

        // Short-circuit: a prior run (possibly while backgrounded) may have
        // already written the file. Report it complete rather than refetch.
        if (dest.exists() && dest.length() > 0) {
            report(DownloadReportWire.completed(episodeId, dest.absolutePath))
            return
        }

        val job = scope.launch {
            try {
                runDownload(episodeId, url, dest, hintBytes)
            } catch (cancel: kotlinx.coroutines.CancellationException) {
                // Cooperative cancel (reconcile removed the row). Clean up the
                // partial file and tell the kernel we stopped.
                partFile(dest).delete()
                if (!detached.get()) report(DownloadReportWire.cancelled(episodeId))
                throw cancel
            } catch (t: Throwable) {
                partFile(dest).delete()
                if (!coroutineContext.isActive && !detached.get()) {
                    report(DownloadReportWire.cancelled(episodeId))
                } else if (!detached.get()) {
                    report(DownloadReportWire.failed(episodeId, t.message ?: "download-failed"))
                }
            } finally {
                inFlight.remove(episodeId)
                inFlightCalls.remove(episodeId)
            }
        }
        inFlight[episodeId] = job
    }

    private suspend fun runDownload(
        episodeId: String,
        url: String,
        dest: File,
        hintBytes: Long?,
    ) {
        val request = Request.Builder().url(url).build()
        val call = client.newCall(request)
        inFlightCalls[episodeId] = call
        call.execute().use { response ->
            if (!response.isSuccessful) {
                report(DownloadReportWire.failed(episodeId, "http-${response.code}"))
                return
            }
            val body = response.body ?: run {
                report(DownloadReportWire.failed(episodeId, "empty-body"))
                return
            }
            val totalBytes = body.contentLength().takeIf { it > 0 } ?: hintBytes
            val part = partFile(dest)
            part.parentFile?.mkdirs()

            var downloaded = 0L
            var lastReportedBytes = 0L
            var lastReportAt = 0L

            body.byteStream().use { input ->
                part.outputStream().use { output ->
                    val buffer = ByteArray(BUFFER_BYTES)
                    while (true) {
                        coroutineContext.ensureActive() // cooperative cancel
                        val read = input.read(buffer)
                        if (read < 0) break
                        output.write(buffer, 0, read)
                        downloaded += read
                        val now = System.currentTimeMillis()
                        if (shouldReportProgress(downloaded, lastReportedBytes, totalBytes, now, lastReportAt)) {
                            report(DownloadReportWire.progress(episodeId, downloaded, totalBytes))
                            lastReportedBytes = downloaded
                            lastReportAt = now
                        }
                    }
                    output.flush()
                }
            }

            // Atomic publish: rename the .part file onto the final path so a
            // mid-write crash never leaves a truncated file the kernel treats
            // as complete.
            if (dest.exists()) dest.delete()
            if (!part.renameTo(dest)) {
                report(DownloadReportWire.failed(episodeId, "rename-failed"))
                return
            }
            report(DownloadReportWire.completed(episodeId, dest.absolutePath))
        }
    }

    /**
     * D8 progress throttle, mirroring iOS: emit only when both gates open —
     * ≥1% of `total_bytes` (or ≥256 KiB when total unknown) AND ≥1 s since
     * the last emit. Keeps the per-report store/queue lock + rev bump off the
     * hot read loop.
     */
    private fun shouldReportProgress(
        downloaded: Long,
        lastReported: Long,
        totalBytes: Long?,
        now: Long,
        lastAt: Long,
    ): Boolean {
        if (now - lastAt < PROGRESS_MIN_INTERVAL_MS) return false
        val delta = downloaded - lastReported
        val byteGate = if (totalBytes != null && totalBytes > 0) {
            delta.toDouble() / totalBytes.toDouble() >= 0.01
        } else {
            delta >= PROGRESS_MIN_DELTA_BYTES
        }
        return byteGate
    }

    /**
     * Forward a `DownloadReport` to the kernel. The kernel projects it onto
     * its `DownloadQueue` (advancing the snapshot the UI reads) and returns a
     * follow-up command we deliberately discard — reconcile is the writer.
     */
    private fun report(reportJson: String) {
        if (detached.get() || !scope.isActive) return
        runCatching { bridge.downloadReport(reportJson) }
            .onFailure { Log.w(TAG, "download report failed", it) }
    }

    /**
     * Final on-disk location. App-private external podcasts dir when
     * available (survives reinstall-cache pressure, large-file friendly),
     * falling back to internal `filesDir/downloads`. The extension is derived
     * from the URL so the file is playable by ExoPlayer's extractors.
     */
    private fun destinationFile(episodeId: String, url: String): File {
        val baseDir = context.getExternalFilesDir(Environment.DIRECTORY_PODCASTS)
            ?: File(context.filesDir, "downloads")
        val ext = extensionFor(url)
        return File(baseDir, "${sanitize(episodeId)}$ext")
    }

    private fun partFile(dest: File): File = File(dest.parentFile, dest.name + ".part")

    companion object {
        private const val TAG = "DownloadCapability"
        private const val STATE_ACTIVE = "active"
        private const val BUFFER_BYTES = 64 * 1024
        private const val PROGRESS_MIN_INTERVAL_MS = 1_000L
        private const val PROGRESS_MIN_DELTA_BYTES = 256L * 1024L

        /** Matches `DOWNLOAD_CAPABILITY_NAMESPACE` in
         *  `apps/nmp-app-podcast/src/capability/download.rs`. Reports route
         *  through the dedicated `downloadReport` channel, so this constant is
         *  documentation of the contract rather than a dispatch key. */
        const val NAMESPACE: String = "nmp.download.capability"

        private fun extensionFor(url: String): String {
            val path = url.substringBefore('?').substringBefore('#')
            val dot = path.lastIndexOf('.')
            val slash = path.lastIndexOf('/')
            if (dot <= slash || dot == path.length - 1) return ".mp3"
            val ext = path.substring(dot).lowercase()
            // Guard against absurd "extensions" from query-less odd URLs.
            return if (ext.length in 2..5 && ext.drop(1).all { it.isLetterOrDigit() }) ext else ".mp3"
        }

        private fun sanitize(episodeId: String): String =
            episodeId.map { if (it.isLetterOrDigit() || it == '-' || it == '_') it else '_' }
                .joinToString("")
    }
}
