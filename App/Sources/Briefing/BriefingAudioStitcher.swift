import AVFoundation
import Foundation

// MARK: - BriefingAudioStitcher

/// Concatenates a list of `BriefingTrack`s into a single playable .m4a using
/// `AVMutableComposition` + `AVAssetExportSession`. The result lives at the
/// path `BriefingStorage` chose for the briefing's stitched audio.
///
/// Stitching is deterministic and order-preserving: track[0] starts at 00:00,
/// track[N] starts at the cumulative end of track[N-1]. No crossfades — the
/// player handles segment-transition motion via `glassEffectID`, not via the
/// audio file itself.
struct BriefingAudioStitcher: Sendable {

    /// Concatenates the supplied tracks into one composition exported to
    /// `outputURL`. Returns the realised total duration so the caller can
    /// stash it on `BriefingScript.totalDurationSeconds`.
    static func stitch(
        tracks: [BriefingTrack],
        outputURL: URL
    ) async throws -> TimeInterval {
        guard !tracks.isEmpty else {
            throw StitchError.noTracks
        }

        // AVMutableComposition is `@unchecked Sendable` in modern SDKs but
        // we keep all uses inside this function so the strict-concurrency
        // checker is happy regardless of OS-level annotations.
        let composition = AVMutableComposition()
        guard let audioTrack = composition.addMutableTrack(
            withMediaType: .audio,
            preferredTrackID: kCMPersistentTrackID_Invalid
        ) else {
            throw StitchError.cannotAllocateTrack
        }

        var cursor: CMTime = .zero
        for track in tracks {
            try await appendTrack(track, into: audioTrack, at: &cursor)
        }

        try? FileManager.default.removeItem(at: outputURL)
        try FileManager.default.createDirectory(
            at: outputURL.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )

        guard let export = AVAssetExportSession(
            asset: composition,
            presetName: AVAssetExportPresetAppleM4A
        ) else {
            throw StitchError.cannotCreateExportSession
        }

        try await runExport(export, to: outputURL)
        return CMTimeGetSeconds(cursor)
    }

    // MARK: Track appending

    private static func appendTrack(
        _ track: BriefingTrack,
        into audioTrack: AVMutableCompositionTrack,
        at cursor: inout CMTime
    ) async throws {
        let asset = AVURLAsset(url: track.audioURL)
        let sourceTracks: [AVAssetTrack]
        do {
            sourceTracks = try await asset.loadTracks(withMediaType: .audio)
        } catch {
            throw StitchError.cannotLoadAsset(track.audioURL, error)
        }
        guard let source = sourceTracks.first else {
            // Source had no audio — stitch a silence pad of the requested
            // duration so the timeline keeps lining up. A dropped track
            // would silently shift every later cue.
            try insertSilence(
                into: audioTrack,
                at: &cursor,
                durationSeconds: track.durationSeconds
            )
            return
        }

        let assetDuration: CMTime
        do {
            assetDuration = try await asset.load(.duration)
        } catch {
            throw StitchError.cannotLoadAsset(track.audioURL, error)
        }

        let startSec = max(0, track.startInTrackSeconds)
        let endSec = min(CMTimeGetSeconds(assetDuration), track.endInTrackSeconds)
        let durationSec = max(0, endSec - startSec)
        guard durationSec > 0 else { return }

        let timescale: CMTimeScale = 600
        let range = CMTimeRange(
            start: CMTime(seconds: startSec, preferredTimescale: timescale),
            duration: CMTime(seconds: durationSec, preferredTimescale: timescale)
        )

        do {
            try audioTrack.insertTimeRange(range, of: source, at: cursor)
        } catch {
            throw StitchError.insertFailed(error)
        }
        cursor = CMTimeAdd(cursor, range.duration)
    }

    /// Inserts `durationSeconds` of silence at `cursor` by writing a temp
    /// silent m4a and splicing it. Used when a quote source can't be loaded
    /// — the spec requires we never drop a citation silently.
    private static func insertSilence(
        into audioTrack: AVMutableCompositionTrack,
        at cursor: inout CMTime,
        durationSeconds: TimeInterval
    ) throws {
        guard durationSeconds > 0 else { return }
        let tmp = FileManager.default.temporaryDirectory
            .appendingPathComponent("briefing-silence-\(UUID().uuidString).m4a")
        try SilentAudioWriter.writeSilence(durationSeconds: durationSeconds, to: tmp)
        defer { try? FileManager.default.removeItem(at: tmp) }

        let asset = AVURLAsset(url: tmp)
        // We just wrote this file synchronously; loadTracks(withMediaType:)
        // is required on iOS 16+ and works fine even on freshly-written
        // assets. Bridge the async API into our throwing-sync context via a
        // semaphore.
        let semaphore = DispatchSemaphore(value: 0)
        nonisolated(unsafe) var loaded: [AVAssetTrack] = []
        Task.detached {
            loaded = (try? await asset.loadTracks(withMediaType: .audio)) ?? []
            semaphore.signal()
        }
        semaphore.wait()
        guard let source = loaded.first else { return }

        let timescale: CMTimeScale = 600
        let range = CMTimeRange(
            start: .zero,
            duration: CMTime(seconds: durationSeconds, preferredTimescale: timescale)
        )
        try audioTrack.insertTimeRange(range, of: source, at: cursor)
        cursor = CMTimeAdd(cursor, range.duration)
    }

    // MARK: Export

    private static func runExport(
        _ export: AVAssetExportSession,
        to outputURL: URL
    ) async throws {
        // iOS 18 introduced `export(to:as:)` async; older code paths used
        // `exportAsynchronously` + `status`. We use the async variant
        // because the project's deployment target is iOS 26.
        do {
            try await export.export(to: outputURL, as: .m4a)
        } catch {
            throw StitchError.exportFailed(error)
        }
    }

    // MARK: Errors

    enum StitchError: LocalizedError {
        case noTracks
        case cannotAllocateTrack
        case cannotCreateExportSession
        case cannotLoadAsset(URL, Error)
        case insertFailed(Error)
        case exportFailed(Error?)
        case cancelled

        var errorDescription: String? {
            switch self {
            case .noTracks:
                "Cannot stitch a briefing with zero tracks."
            case .cannotAllocateTrack:
                "Could not allocate an audio track in the composition."
            case .cannotCreateExportSession:
                "Could not create export session for stitched briefing."
            case .cannotLoadAsset(let url, let err):
                "Could not load \(url.lastPathComponent): \(err.localizedDescription)"
            case .insertFailed(let err):
                "Track insert failed: \(err.localizedDescription)"
            case .exportFailed(let err):
                "Export failed: \(err?.localizedDescription ?? "unknown")"
            case .cancelled:
                "Stitching cancelled."
            }
        }
    }
}
