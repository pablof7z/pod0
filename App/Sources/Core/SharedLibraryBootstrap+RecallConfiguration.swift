import Foundation
import Pod0Core

extension SharedLibraryBootstrap {
    static func importLegacyRecallConfiguration(
        _ legacy: LegacyRecallConfigurationSeed?,
        into facade: Pod0Facade
    ) throws {
        if let legacy {
            let source = [
                legacy.storedEmbeddingModelID,
                legacy.rerankerEnabled ? "1" : "0",
            ].joined(separator: "\u{1f}")
            facade.dispatch(command: CommandEnvelope(
                commandId: CommandId(uuid: UUID()),
                cancellationId: CancellationId(uuid: UUID()),
                expectedRevision: nil,
                command: .importLegacyRecallConfiguration(
                    configuration: RecallConfigurationInput(
                        storedEmbeddingModelId: legacy.storedEmbeddingModelID,
                        rerankerEnabled: legacy.rerankerEnabled
                    ),
                    sourceGeneration: stableDigest("pod0-recall-config:\(source)")
                )
            ))
        }
        guard case .recallConfiguration = facade.snapshot(request: ProjectionRequest(
            scope: .recallConfiguration,
            offset: 0,
            maxItems: 1
        )).projection else {
            throw SharedLibraryBootstrapError.verificationFailed
        }
    }
}
