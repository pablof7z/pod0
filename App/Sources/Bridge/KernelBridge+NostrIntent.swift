import Foundation

extension PodcastHandle {
    func classifyNostrIntent(
        input: String,
        scopes: [NostrIntentScope],
        textTargets: NostrIntentTextTargets = .userPreferred
    ) -> NostrIntentClassificationEnvelope? {
        let request = NostrIntentRequest(
            input: input,
            scopes: scopes,
            textTargets: textTargets
        )
        guard let json = request.jsonString() else { return nil }
        return NostrIntentClassificationEnvelope.decode(json:
            podcastApp.classifyInputIntent(requestJson: json)
        )
    }

    func dispatchNostrIntent(
        input: String,
        scopes: [NostrIntentScope],
        sessionID: String,
        textTargets: NostrIntentTextTargets = .userPreferred
    ) -> NostrIntentDispatchOutcome? {
        let request = NostrIntentRequest(
            input: input,
            scopes: scopes,
            textTargets: textTargets
        )
        guard let json = request.jsonString() else { return nil }
        return NostrIntentDispatchOutcome.decode(json:
            podcastApp.dispatchInputIntent(requestJson: json, sessionId: sessionID)
        )
    }

    func decodeNostrRef(uri: String) -> DecodedNostrRefTarget? {
        DecodedNostrRefTarget.decode(json: podcastApp.decodeNip21Uri(input: uri))
    }
}
