package io.f7z.podcast.capabilities

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject

/**
 * Report wire vocabulary for the download capability — the Kotlin encoder
 * for the Rust `DownloadReport` enum in
 * `apps/nmp-app-podcast/src/capability/download.rs`
 * (`#[serde(tag = "type", rename_all = "snake_case")]`).
 *
 * Only the *report* direction (Kotlin → kernel) is modelled. The capability
 * deliberately ignores the follow-up `DownloadCommand` the kernel returns
 * from `downloadReport`: the snapshot reconcile loop is the **single
 * writer** that starts and cancels downloads, so acting on the returned
 * command too would race it (and the FFI return carries only one command
 * while `handle_report` may emit several). The report call is still made for
 * its side effect — projecting onto the kernel `DownloadQueue` and bumping
 * the snapshot rev.
 *
 * JSON is hand-built with `JsonObject` (same approach as
 * `ExoPlayerCapability` for `AudioReport`) so the wire shape is explicit and
 * byte-identical to serde's output.
 */
internal object DownloadReportWire {
    private val json: Json = Json { encodeDefaults = true }

    /**
     * `{"type":"progress","episode_id":…,"bytes_downloaded":N,"total_bytes":M}`
     * `total_bytes` is omitted when unknown, matching the Rust
     * `#[serde(skip_serializing_if = "Option::is_none")]`.
     */
    fun progress(episodeId: String, bytesDownloaded: Long, totalBytes: Long?): String =
        encode(buildJsonObject {
            put("type", JsonPrimitive("progress"))
            put("episode_id", JsonPrimitive(episodeId))
            put("bytes_downloaded", JsonPrimitive(bytesDownloaded))
            if (totalBytes != null && totalBytes > 0) {
                put("total_bytes", JsonPrimitive(totalBytes))
            }
        })

    /** `{"type":"completed","episode_id":…,"local_path":…}` */
    fun completed(episodeId: String, localPath: String): String =
        encode(buildJsonObject {
            put("type", JsonPrimitive("completed"))
            put("episode_id", JsonPrimitive(episodeId))
            put("local_path", JsonPrimitive(localPath))
        })

    /** `{"type":"failed","episode_id":…,"error":…}` */
    fun failed(episodeId: String, error: String): String =
        encode(buildJsonObject {
            put("type", JsonPrimitive("failed"))
            put("episode_id", JsonPrimitive(episodeId))
            put("error", JsonPrimitive(error))
        })

    /** `{"type":"cancelled","episode_id":…}` */
    fun cancelled(episodeId: String): String =
        encode(buildJsonObject {
            put("type", JsonPrimitive("cancelled"))
            put("episode_id", JsonPrimitive(episodeId))
        })

    private fun encode(obj: JsonObject): String =
        json.encodeToString(JsonObject.serializer(), obj)
}
