import Foundation
import Pod0Core

struct CoreDownloadTaskIdentity: Codable, Equatable, Sendable {
    private static let schemaVersion: UInt8 = 1

    let version: UInt8
    let requestHigh: UInt64
    let requestLow: UInt64
    let cancellationHigh: UInt64
    let cancellationLow: UInt64
    let issuedRevision: UInt64
    let episodeHigh: UInt64
    let episodeLow: UInt64
    let intentHigh: UInt64
    let intentLow: UInt64
    let attemptHigh: UInt64
    let attemptLow: UInt64
    let inputVersion: String

    init?(_ envelope: HostRequestEnvelope) {
        guard case let .startEpisodeDownload(
            episodeID,
            intentID,
            attemptID,
            inputVersion,
            _,
            _
        ) = envelope.request else { return nil }
        self.init(
            requestID: envelope.requestId,
            cancellationID: envelope.cancellationId,
            issuedRevision: envelope.issuedRevision,
            episodeID: episodeID,
            intentID: intentID,
            attemptID: attemptID,
            inputVersion: inputVersion
        )
    }

    init(
        requestID: HostRequestId,
        cancellationID: CancellationId,
        issuedRevision: StateRevision = StateRevision(value: 0),
        episodeID: EpisodeId,
        intentID: DownloadIntentId,
        attemptID: DownloadAttemptId,
        inputVersion: String
    ) {
        version = Self.schemaVersion
        requestHigh = requestID.high
        requestLow = requestID.low
        cancellationHigh = cancellationID.high
        cancellationLow = cancellationID.low
        self.issuedRevision = issuedRevision.value
        episodeHigh = episodeID.high
        episodeLow = episodeID.low
        intentHigh = intentID.high
        intentLow = intentID.low
        attemptHigh = attemptID.high
        attemptLow = attemptID.low
        self.inputVersion = inputVersion
    }

    var requestID: HostRequestId { HostRequestId(high: requestHigh, low: requestLow) }
    var cancellationID: CancellationId {
        CancellationId(high: cancellationHigh, low: cancellationLow)
    }
    var observedRequestRevision: StateRevision { StateRevision(value: issuedRevision) }
    var episodeID: EpisodeId { EpisodeId(high: episodeHigh, low: episodeLow) }
    var intentID: DownloadIntentId { DownloadIntentId(high: intentHigh, low: intentLow) }
    var attemptID: DownloadAttemptId { DownloadAttemptId(high: attemptHigh, low: attemptLow) }

    var encoded: String? {
        guard let data = try? JSONEncoder().encode(self) else { return nil }
        return "pod0-download-v1:" + data.base64EncodedString()
    }

    init?(encoded value: String?) {
        let prefix = "pod0-download-v1:"
        guard let value, value.hasPrefix(prefix),
              let data = Data(base64Encoded: String(value.dropFirst(prefix.count))),
              let decoded = try? JSONDecoder().decode(Self.self, from: data),
              decoded.version == Self.schemaVersion,
              !decoded.inputVersion.isEmpty,
              decoded.inputVersion.utf8.count <= 256
        else { return nil }
        self = decoded
    }

    var stableAttemptKey: String {
        String(format: "%016llx%016llx", attemptHigh, attemptLow)
    }
}
