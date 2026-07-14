import AVFoundation
import Foundation
import os.log

// MARK: - Adapters and shared utilities
//
// This file used to be wall-to-wall fakes. With Lanes 6/7/8 wired up the
// production composer now relies on:
//   - `SilentAudioWriter` (a small AAC scratch writer, also reused by the
//     audio stitcher when an original-quote enclosure can't be downloaded)
//   - `ElevenLabsBriefingTTS` (the real TTS adapter — wraps Lane 8's
//     streaming client behind the file-oriented `TTSProtocol`)
//   - `FakeBriefingPlayerHost` (the only available `BriefingPlayerHostProtocol`
//     implementation today — Lane 1's `AudioEngine`-as-host hasn't shipped
//     yet, and `BriefingPlayerView` constructs one of these in production)
//
// The remaining `Fake*` data sources (`FakeRAGSearch`, `FakeWikiStorage`,
// `FakeTTS`) are now `#if DEBUG`-only so production code paths can't reach
// them. They stay around for previews and tests.

// MARK: Silent audio writer

/// Writes a silent AAC m4a of `duration` seconds to `url`. Used by
/// `BriefingAudioStitcher` whenever an original-quote enclosure cannot be
/// resolved (the stitcher substitutes silence so the timeline still lines
/// up) and by the debug `FakeTTS` provider for SwiftUI previews / tests.
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

// MARK: - ElevenLabs briefing TTS adapter

/// Production `TTSProtocol` adapter. Wraps `ElevenLabsTTSClient`'s streaming
/// surface (which yields raw audio frames over an `AsyncThrowingStream`)
/// behind the file-oriented contract the briefing composer expects.
///
/// Behaviour:
///   1. Validates that `ElevenLabsCredentialStore` has an API key. Throws
///      `AdapterError.missingCredentials` when not. The composer surfaces
///      that as a Settings prompt rather than a synthesise failure.
///   2. Spools every byte from the stream into `outputURL` via `FileHandle`
///      so a multi-minute briefing never holds the whole clip in memory.
///   3. Loads the resulting file as an `AVURLAsset` and reads the realised
///      `.duration` so the composer can populate `BriefingTrack.endInTrack`
///      against the *real* clip length, not a wpm estimate.
///
/// The output filename uses the composer's `.m4a` convention. AVFoundation
/// sniffs the container from the leading bytes, so the REST fallback's MP3
/// payload still plays through `AVMutableComposition` without rewrap.
struct ElevenLabsBriefingTTS: TTSProtocol {

    enum AdapterError: Error, Sendable, Equatable {
        case missingCredentials
        case streamProducedNoAudio
        case durationLoadFailed(String)
    }

    private static let logger = Logger.app("ElevenLabsBriefingTTS")
    let client: ElevenLabsTTSClient

    init(client: ElevenLabsTTSClient = ElevenLabsTTSClient()) {
        self.client = client
    }

    func synthesize(
        text: String,
        voiceID: String,
        outputURL: URL
    ) async throws -> TimeInterval {
        guard client.isConfigured else {
            throw AdapterError.missingCredentials
        }

        try? FileManager.default.removeItem(at: outputURL)
        try FileManager.default.createDirectory(
            at: outputURL.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )

        let resolvedVoiceID = voiceID.isEmpty ? ElevenLabsTTSClient.defaultVoiceID : voiceID
        let stream = client.synthesizeStream(text: text, voiceID: resolvedVoiceID)

        FileManager.default.createFile(atPath: outputURL.path, contents: nil)
        guard let handle = try? FileHandle(forWritingTo: outputURL) else {
            throw AdapterError.streamProducedNoAudio
        }
        defer { try? handle.close() }

        var totalBytes = 0
        for try await chunk in stream {
            try handle.write(contentsOf: chunk)
            totalBytes += chunk.count
        }

        guard totalBytes > 0 else {
            throw AdapterError.streamProducedNoAudio
        }

        let asset = AVURLAsset(url: outputURL)
        do {
            let duration = try await asset.load(.duration)
            return CMTimeGetSeconds(duration)
        } catch {
            Self.logger.error("Failed to load TTS asset duration: \(error.localizedDescription, privacy: .public)")
            throw AdapterError.durationLoadFailed(error.localizedDescription)
        }
    }
}

// MARK: - Fake briefing player host

/// In-memory `AVPlayer`-backed host used by `BriefingPlayerView`. Lane 1's
/// `AudioEngine` will eventually expose a `BriefingPlayerHostProtocol`
/// conformer that hands the stitched .m4a into the same Now-Playing /
/// CarPlay surface the regular podcast player uses; until then this host
/// keeps the player UI fully exercised.
@MainActor
final class FakeBriefingPlayerHost: BriefingPlayerHostProtocol {
    private var player: AVPlayer?
    /// Item-scoped end-of-stream observer. Held so we can remove it on
    /// re-entry — otherwise back-to-back `play(assetURL:)` calls (e.g.
    /// `BriefingRiverView` advancing between briefings) would stack
    /// observers and fire the callback N times for the Nth briefing.
    private var endObserver: NSObjectProtocol?

    var onPlaybackEnded: (@MainActor () -> Void)?

    var currentTimeSeconds: TimeInterval {
        guard let player else { return 0 }
        return CMTimeGetSeconds(player.currentTime())
    }

    func play(assetURL: URL, startAt seconds: TimeInterval) async {
        let item = AVPlayerItem(url: assetURL)
        // Tear down any prior observer; the new item replaces the old.
        if let endObserver {
            NotificationCenter.default.removeObserver(endObserver)
        }
        // Scope to `object: item` — without it every AVPlayerItem in the
        // process fires this callback (podcast player, voice mode, etc.).
        endObserver = NotificationCenter.default.addObserver(
            forName: .AVPlayerItemDidPlayToEndTime,
            object: item,
            queue: .main
        ) { [weak self] _ in
            // Hop to MainActor — the notification arrives on the main queue
            // (per the operationQueue arg) but the closure is `@Sendable`,
            // so Swift 6 strict concurrency won't let us call the actor-
            // isolated `onPlaybackEnded` directly without the assumption.
            MainActor.assumeIsolated {
                self?.onPlaybackEnded?()
            }
        }
        let player = AVPlayer(playerItem: item)
        self.player = player
        if seconds > 0 {
            await player.seek(to: CMTime(seconds: seconds, preferredTimescale: 600))
        }
        player.play()
    }

    func pause() async { player?.pause() }
    func resume() async { player?.play() }
    func seek(to seconds: TimeInterval) async {
        await player?.seek(to: CMTime(seconds: seconds, preferredTimescale: 600))
    }

    // No deinit cleanup: the closure captures `[weak self]`, so once the host
    // deallocates the callback is a no-op and the observer becomes harmless
    // bookkeeping that NotificationCenter releases naturally. Swift 6's
    // nonisolated-deinit rule blocks touching `endObserver` here anyway.
}

// MARK: - Debug-only data fakes
//
// Available to SwiftUI previews and unit tests only. Production code paths
// must not reference these — the composer now defaults to real RAG / wiki /
// TTS dependencies (see `BriefingComposer.init`). Gating with `#if DEBUG`
// gives the test target access (tests build the app in Debug) while
// guaranteeing a Release build cannot accidentally fall back to fixture data.

#if DEBUG

// MARK: Fake TTS

/// Synthesises silent m4a files at the requested duration so the briefing
/// pipeline can compose real audio assets without an ElevenLabs key.
struct FakeTTS: TTSProtocol {
    /// Approximate words-per-minute the fake uses to estimate narration
    /// duration from text length. Matches a measured ElevenLabs Multilingual
    /// v2 narrator in the *brassAmber* register (~155 wpm).
    var wordsPerMinute: Double = 155

    func synthesize(text: String, voiceID _: String, outputURL: URL) async throws -> TimeInterval {
        let wordCount = max(1, text.split(whereSeparator: \.isWhitespace).count)
        let duration = max(2.0, Double(wordCount) / wordsPerMinute * 60.0)
        try SilentAudioWriter.writeSilence(durationSeconds: duration, to: outputURL)
        return duration
    }
}

// MARK: Fake RAG

/// Returns a small, deterministic fixture set so the composer always has
/// something to compose against. Contents are seeded from `query` so unit
/// tests can assert on stable ids.
struct FakeRAGSearch: BriefingRAGSearchProtocol {
    func search(query: String, scope _: BriefingScope, limit: Int) async throws -> [RAGCandidate] {
        let seeds: [(showName: String, startSec: Double, snippet: String)] = [
            ("Hard Fork", 2052, "Sundar mentioned a new TPU this week."),
            ("Huberman Lab", 1084, "Ozempic suppresses appetite signaling."),
            ("Peter Attia Drive", 1367, "GLP-1 receptor agonists and longevity."),
            ("The Verge cast", 730, "Google's AI strategy across the cast."),
            ("Lex Fridman", 2853, "Backlog highlights from the past month."),
        ]
        return seeds.prefix(limit).enumerated().map { (index, seed) in
            let showName = seed.showName
            let start = seed.startSec
            let snippet = seed.snippet
            return RAGCandidate(
                id: deterministicID(from: "\(query)|\(index)"),
                sourceKind: .episode,
                episodeID: deterministicID(from: "episode|\(showName)"),
                enclosureURL: URL(string: "https://example.com/feeds/\(index).mp3"),
                wikiPageID: nil,
                sourceLabel: "\(showName) · \(formatTime(start))",
                text: snippet,
                startSeconds: start,
                endSeconds: start + 15,
                score: 1.0 - Double(index) * 0.07
            )
        }
    }

    private func formatTime(_ seconds: TimeInterval) -> String {
        let mm = Int(seconds) / 60
        let ss = Int(seconds) % 60
        return String(format: "%d:%02d", mm, ss)
    }

    private func deterministicID(from input: String) -> UUID {
        var hasher = Hasher()
        hasher.combine(input)
        let hash = UInt64(bitPattern: Int64(hasher.finalize()))
        var bytes = [UInt8](repeating: 0, count: 16)
        for i in 0..<16 {
            let shift = UInt64(i % 8) * 8
            bytes[i] = UInt8((hash >> shift) & 0xFF)
        }
        let tuple: uuid_t = (
            bytes[0], bytes[1], bytes[2], bytes[3],
            bytes[4], bytes[5], bytes[6], bytes[7],
            bytes[8], bytes[9], bytes[10], bytes[11],
            bytes[12], bytes[13], bytes[14], bytes[15]
        )
        return UUID(uuid: tuple)
    }
}

// MARK: Fake wiki storage

/// In-memory wiki store seeded with a couple of pages so topic-deep-dive
/// briefings have a structural backbone in dev / preview builds.
struct FakeWikiStorage: BriefingWikiStorageProtocol {
    var pages: [WikiPage]

    init(pages: [WikiPage]? = nil) {
        self.pages = pages ?? [
            WikiPage(slug: "ozempic", title: "Ozempic", kind: .topic, scope: .global, summary: "GLP-1 receptor agonist class, originally for type 2 diabetes."),
            WikiPage(slug: "google-tpu", title: "Google TPU", kind: .topic, scope: .global, summary: "Tensor Processing Unit family, custom ASIC for ML workloads."),
        ]
    }

    func wikiPage(id: UUID) async throws -> WikiPage? {
        pages.first { $0.id == id }
    }

    func wikiPages(matchingTitle titleQuery: String) async throws -> [WikiPage] {
        let needle = titleQuery.lowercased()
        return pages.filter { $0.title.lowercased().contains(needle) }
    }
}

#endif
