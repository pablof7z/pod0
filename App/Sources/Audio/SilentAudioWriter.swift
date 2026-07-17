import AVFoundation
import Foundation

// MARK: - Silent audio writer

/// Writes a silent AAC m4a of `duration` seconds to `url`. Used by
/// `ComposedAudioStitcher` whenever a source track cannot be resolved (the
/// stitcher substitutes silence so the timeline still lines up) and by
/// debug fakes for SwiftUI previews / tests.
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
