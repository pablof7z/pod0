import Pod0Core
import XCTest
@testable import Podcastr

final class NativeTranscriptWorkflowConfigurationTests: XCTestCase {
    func testAutomaticOpportunityRequiresAnExecutablePath() {
        let episode = makeEpisode(publisherTranscriptURL: nil)

        XCTAssertFalse(hasOpportunity(episode, publisher: false, provider: false, credential: true))
        XCTAssertFalse(hasOpportunity(episode, publisher: true, provider: false, credential: true))
        XCTAssertFalse(hasOpportunity(episode, publisher: false, provider: true, credential: false))
        XCTAssertTrue(hasOpportunity(episode, publisher: false, provider: true, credential: true))
    }

    func testPublisherOpportunityRequiresPublisherMetadata() {
        XCTAssertFalse(hasOpportunity(
            makeEpisode(publisherTranscriptURL: nil),
            publisher: true,
            provider: false,
            credential: false
        ))
        XCTAssertTrue(hasOpportunity(
            makeEpisode(publisherTranscriptURL: URL(string: "https://example.com/transcript.vtt")),
            publisher: true,
            provider: false,
            credential: false
        ))
    }

    func testReadyTranscriptSuppressesAutomaticOpportunity() {
        var episode = makeEpisode(publisherTranscriptURL: URL(string: "https://example.com/transcript.vtt"))
        episode.transcriptState = .ready(source: .publisher)

        XCTAssertFalse(hasOpportunity(episode, publisher: true, provider: true, credential: true))
    }

    private func hasOpportunity(
        _ episode: Episode,
        publisher: Bool,
        provider: Bool,
        credential: Bool
    ) -> Bool {
        NativeTranscriptWorkflowConfiguration.hasAutomaticExecutionOpportunity(
            for: episode,
            configuration: TranscriptWorkflowConfiguration(
                provider: .assemblyAi,
                model: "test",
                localAudioUrl: nil,
                credentialAvailable: credential,
                autoPublisherEnabled: publisher,
                autoProviderEnabled: provider
            )
        )
    }

    private func makeEpisode(publisherTranscriptURL: URL?) -> Episode {
        Episode(
            podcastID: UUID(),
            guid: UUID().uuidString,
            title: "Episode",
            pubDate: Date(timeIntervalSince1970: 1_700_000_000),
            enclosureURL: URL(string: "https://example.com/audio.mp3")!,
            publisherTranscriptURL: publisherTranscriptURL
        )
    }
}
