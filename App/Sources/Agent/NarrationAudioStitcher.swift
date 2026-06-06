import AVFoundation
import Foundation

// MARK: - NarrationAudioStitcher

/// Concatenates a list of `NarrationTrack`s into a single playable .m4a using
/// `AVMutableComposition` + `AVAssetExportSession`.
///
/// Stitching is deterministic and order-preserving: track[0] starts at 00:00,
/// track[N] starts at the cumulative end of track[N-1]. No crossfades.
struct NarrationAudioStitcher: Sendable {

    /// Concatenates the supplied tracks into one composition exported to
    /// `outputURL`. Returns the realised total duration.
    static func stitch(
        tracks: [NarrationTrack],
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
        _ track: NarrationTrack,
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
    /// so a citation is never dropped silently.
    private static func insertSilence(
        into audioTrack: AVMutableCompositionTrack,
        at cursor: inout CMTime,
        durationSeconds: TimeInterval
    ) throws {
        guard durationSeconds > 0 else { return }
        let tmp = FileManager.default.temporaryDirectory
            .appendingPathComponent("narration-silence-\(UUID().uuidString).m4a")
        try SilentAudioWriter.writeSilence(durationSeconds: durationSeconds, to: tmp)
        defer { try? FileManager.default.removeItem(at: tmp) }

        let asset = AVURLAsset(url: tmp)
        // We just wrote this file synchronously; loadTracks(withMediaType:)
        // is required on iOS 16+ and works fine even on freshly-written
        // assets. Bridge the async API into our throwing-sync context via a
        // semaphore.
        let semaphore = DispatchSemaphore(value: 0)
        nonisolated(unsafe) var loaded: [AVAssetTrack] = []
        Task.detached { @Sendable in
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
        // iOS 18 introduced `export(to:as:)` async; the project's deployment
        // target is iOS 26 so we use the async variant.
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
                "Cannot stitch audio with zero tracks."
            case .cannotAllocateTrack:
                "Could not allocate an audio track in the composition."
            case .cannotCreateExportSession:
                "Could not create export session for stitched audio."
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

// MARK: - SilentAudioWriter

/// Writes a silent AAC m4a of `duration` seconds to `url`. Used by
/// `NarrationAudioStitcher` whenever a quote enclosure cannot be resolved
/// (the stitcher substitutes silence so the timeline still lines up).
///
/// The output is a real, decoded-audio-equivalent .m4a — `AVPlayer`,
/// `AVMutableComposition`, and `AVAssetExportSession` all consume it without
/// special-case handling.
enum SilentAudioWriter {

    /// AAC-LC at 44.1 kHz mono. Mono is enough for narration and halves disk
    /// footprint vs. stereo; downstream stitching upmixes if a quote happens
    /// to be stereo (AVMutableComposition handles channel-count mismatch).
    private static let sampleRate: Double = 44_100
    private static let channels: Int = 1

    /// Synchronously writes a silent m4a. Throws on file system / writer
    /// errors. AVAssetWriter is not `Sendable`, so the implementation
    /// confines all writer references to this single stack frame and uses a
    /// blocking spin-wait rather than a `requestMediaDataWhenReady` callback
    /// (which would capture non-Sendable state across an `@Sendable` closure).
    static func writeSilence(
        durationSeconds: TimeInterval,
        to url: URL
    ) throws {
        try? FileManager.default.removeItem(at: url)
        try FileManager.default.createDirectory(
            at: url.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )

        let writer = try AVAssetWriter(outputURL: url, fileType: .m4a)
        let settings: [String: Any] = [
            AVFormatIDKey: kAudioFormatMPEG4AAC,
            AVSampleRateKey: sampleRate,
            AVNumberOfChannelsKey: channels,
            AVEncoderBitRateKey: 64_000,
        ]
        let input = AVAssetWriterInput(mediaType: .audio, outputSettings: settings)
        input.expectsMediaDataInRealTime = false
        guard writer.canAdd(input) else {
            throw NSError(
                domain: "SilentAudioWriter",
                code: 1,
                userInfo: [NSLocalizedDescriptionKey: "Writer cannot accept input"]
            )
        }
        writer.add(input)

        guard writer.startWriting() else {
            throw writer.error ?? NSError(domain: "SilentAudioWriter", code: 2)
        }
        writer.startSession(atSourceTime: .zero)

        let totalFrames = Int(durationSeconds * sampleRate)
        let framesPerBuffer = 1_024
        var framesWritten = 0
        while framesWritten < totalFrames {
            // Spin until the encoder accepts more data. AAC encoding is
            // fast enough that this loop yields negligible wall-time.
            while !input.isReadyForMoreMediaData {
                Thread.sleep(forTimeInterval: 0.001)
            }
            let frames = min(framesPerBuffer, totalFrames - framesWritten)
            guard let buffer = makeSilentBuffer(frameCount: frames, startFrame: framesWritten) else {
                break
            }
            if !input.append(buffer) { break }
            framesWritten += frames
        }
        input.markAsFinished()

        let finishSemaphore = DispatchSemaphore(value: 0)
        writer.finishWriting { finishSemaphore.signal() }
        finishSemaphore.wait()

        if writer.status != .completed {
            throw writer.error ?? NSError(domain: "SilentAudioWriter", code: 3)
        }
    }

    private static func makeSilentBuffer(frameCount: Int, startFrame: Int) -> CMSampleBuffer? {
        var formatDesc: CMAudioFormatDescription?
        var asbd = AudioStreamBasicDescription(
            mSampleRate: sampleRate,
            mFormatID: kAudioFormatLinearPCM,
            mFormatFlags: kLinearPCMFormatFlagIsSignedInteger | kLinearPCMFormatFlagIsPacked,
            mBytesPerPacket: UInt32(2 * channels),
            mFramesPerPacket: 1,
            mBytesPerFrame: UInt32(2 * channels),
            mChannelsPerFrame: UInt32(channels),
            mBitsPerChannel: 16,
            mReserved: 0
        )
        CMAudioFormatDescriptionCreate(
            allocator: kCFAllocatorDefault,
            asbd: &asbd,
            layoutSize: 0,
            layout: nil,
            magicCookieSize: 0,
            magicCookie: nil,
            extensions: nil,
            formatDescriptionOut: &formatDesc
        )
        guard let formatDesc else { return nil }

        let byteCount = frameCount * 2 * channels
        var blockBuffer: CMBlockBuffer?
        guard CMBlockBufferCreateWithMemoryBlock(
            allocator: kCFAllocatorDefault,
            memoryBlock: nil,
            blockLength: byteCount,
            blockAllocator: nil,
            customBlockSource: nil,
            offsetToData: 0,
            dataLength: byteCount,
            flags: kCMBlockBufferAssureMemoryNowFlag,
            blockBufferOut: &blockBuffer
        ) == kCMBlockBufferNoErr, let blockBuffer else { return nil }

        // The block is uninitialised memory — zero it so we emit silence.
        CMBlockBufferFillDataBytes(with: 0, blockBuffer: blockBuffer, offsetIntoDestination: 0, dataLength: byteCount)

        var sampleBuffer: CMSampleBuffer?
        let pts = CMTime(value: CMTimeValue(startFrame), timescale: CMTimeScale(sampleRate))
        var timing = CMSampleTimingInfo(
            duration: CMTime(value: 1, timescale: CMTimeScale(sampleRate)),
            presentationTimeStamp: pts,
            decodeTimeStamp: .invalid
        )
        var sampleSize = 2 * channels
        CMSampleBufferCreateReady(
            allocator: kCFAllocatorDefault,
            dataBuffer: blockBuffer,
            formatDescription: formatDesc,
            sampleCount: CMItemCount(frameCount),
            sampleTimingEntryCount: 1,
            sampleTimingArray: &timing,
            sampleSizeEntryCount: 1,
            sampleSizeArray: &sampleSize,
            sampleBufferOut: &sampleBuffer
        )
        return sampleBuffer
    }
}
