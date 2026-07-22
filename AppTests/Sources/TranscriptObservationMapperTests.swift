import Pod0Core
import XCTest
@testable import Podcastr

final class TranscriptObservationMapperTests: XCTestCase {
    func testMapperPreservesTimingOverlapSpeakersAndOptionalWords() throws {
        let speaker = Speaker(
            id: UUID(uuidString: "11111111-2222-3333-4444-555555555555")!,
            label: "spk_0",
            displayName: "Ada"
        )
        let transcript = Transcript(
            episodeID: UUID(),
            language: "en-US",
            source: .scribeV1,
            segments: [
                Segment(
                    start: 0.0004,
                    end: 2,
                    speakerID: speaker.id,
                    text: "First",
                    words: [Word(start: 0.0014, end: 0.0015, text: "First")]
                ),
                Segment(start: 1, end: 3, speakerID: speaker.id, text: "Overlap", words: nil),
            ],
            speakers: [speaker],
            generatedAt: Date(timeIntervalSince1970: 1_700_000_000.125)
        )
        let mapped = try TranscriptObservationMapper.map(
            transcript,
            context: context(podcastID: UUID())
        )

        XCTAssertEqual(mapped.segments[0].startMilliseconds, 0)
        XCTAssertEqual(mapped.segments[0].words[0].startMilliseconds, 1)
        XCTAssertEqual(mapped.segments[0].words[0].endMilliseconds, 2)
        XCTAssertEqual(mapped.segments[1].startMilliseconds, 1_000)
        XCTAssertTrue(mapped.segments[1].words.isEmpty)
        XCTAssertEqual(
            mapped.speakers[0].speakerId,
            transcriptSpeakerId(
                episodeId: EpisodeId(uuid: transcript.episodeID),
                sourceRevision: "audio-v1",
                label: speaker.label
            )
        )
        XCTAssertEqual(mapped.segments[0].speakerId, mapped.speakers[0].speakerId)
        XCTAssertEqual(mapped.speakers[0].label, "spk_0")
        XCTAssertEqual(mapped.speakers[0].displayName, "Ada")
        XCTAssertEqual(mapped.provider, "elevenLabsScribe")
        XCTAssertEqual(mapped.generatedAt.value, 1_700_000_000_125)
    }

    func testEveryNativeSourceMapsToTypedCoreSourceAndProvider() throws {
        let cases: [(Podcastr.TranscriptSource, Pod0Core.TranscriptSource, String?)] = [
            (.publisher, .publisher, nil),
            (.scribeV1, .scribe, "elevenLabsScribe"),
            (.whisper, .whisper, "openRouterWhisper"),
            (.onDevice, .onDevice, "appleSpeech"),
            (.assemblyAI, .assemblyAi, "assemblyAI"),
        ]
        for (source, expectedSource, expectedProvider) in cases {
            let transcript = Transcript(
                episodeID: UUID(),
                language: "en",
                source: source,
                segments: [Segment(start: 0, end: 1, text: "Typed observation")]
            )
            let mapped = try TranscriptObservationMapper.map(
                transcript,
                context: context(podcastID: UUID())
            )
            XCTAssertEqual(mapped.source, expectedSource)
            XCTAssertEqual(mapped.provider, expectedProvider)
        }
    }

    func testProviderOverrideAndDigestParsingAreStable() throws {
        let uppercase = String(repeating: "AB", count: 32)
        let transcript = Transcript(
            episodeID: UUID(), language: "en", source: .onDevice,
            segments: [Segment(start: 0, end: 1, text: "Generated")]
        )
        let mapped = try TranscriptObservationMapper.map(
            transcript,
            context: TranscriptObservationContext(
                podcastID: UUID(), sourceRevision: "agent-v1",
                sourcePayloadDigest: uppercase, provider: "pod0AgentComposer"
            )
        )
        XCTAssertEqual(mapped.provider, "pod0AgentComposer")
        XCTAssertEqual(mapped.sourcePayloadDigest.stableString, uppercase.lowercased())
    }

    func testInvalidTimingAndDigestFailBeforeFFICommit() {
        XCTAssertThrowsError(try TranscriptObservationMapper.milliseconds(.nan))
        XCTAssertThrowsError(try TranscriptObservationMapper.milliseconds(-0.1))
        let transcript = Transcript(
            episodeID: UUID(), language: "en", source: .publisher,
            segments: [Segment(start: 0, end: 1, text: "Invalid digest")]
        )
        XCTAssertThrowsError(try TranscriptObservationMapper.map(
            transcript,
            context: TranscriptObservationContext(
                podcastID: UUID(), sourceRevision: "v1",
                sourcePayloadDigest: "not-a-digest", provider: nil
            )
        ))
    }

    func testSpeakerIdentityIgnoresProviderGeneratedUUIDsAndRejectsUnknownReferences() throws {
        let episodeID = UUID()
        func mappedSpeaker(nativeID: UUID) throws -> SpeakerId {
            let speaker = Speaker(id: nativeID, label: "speaker-0", displayName: nil)
            let transcript = Transcript(
                episodeID: episodeID,
                language: "en",
                source: .assemblyAI,
                segments: [Segment(start: 0, end: 1, speakerID: nativeID, text: "Stable")],
                speakers: [speaker]
            )
            return try TranscriptObservationMapper.map(
                transcript,
                context: context(podcastID: UUID())
            ).speakers[0].speakerId
        }
        XCTAssertEqual(try mappedSpeaker(nativeID: UUID()), try mappedSpeaker(nativeID: UUID()))

        let transcript = Transcript(
            episodeID: episodeID,
            language: "en",
            source: .assemblyAI,
            segments: [Segment(start: 0, end: 1, speakerID: UUID(), text: "Unknown")]
        )
        XCTAssertThrowsError(try TranscriptObservationMapper.map(
            transcript,
            context: context(podcastID: UUID())
        )) { error in
            XCTAssertEqual(error as? TranscriptObservationMappingError, .unknownSpeakerReference)
        }
    }

    func testFutureAndOtherCoreSourcesFailClosed() {
        XCTAssertThrowsError(try SharedTranscriptReader.nativeSource(.other))
        XCTAssertThrowsError(
            try SharedTranscriptReader.nativeSource(.unsupported(wireCode: 9_999))
        )
    }

    private func context(podcastID: UUID) -> TranscriptObservationContext {
        TranscriptObservationContext(
            podcastID: podcastID,
            sourceRevision: "audio-v1",
            sourcePayloadDigest: String(repeating: "01", count: 32),
            provider: nil
        )
    }
}
