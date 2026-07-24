import Foundation
import Pod0Core

extension SharedLibraryClient {
    func dispatchCoreCommand(
        _ command: ApplicationCommand,
        commandID: CommandId = CommandId(uuid: UUID()),
        cancellationID: CancellationId = CancellationId(uuid: UUID()),
        drainHostRequests: Bool = true
    ) {
        let envelope = CommandEnvelope(
            commandId: commandID,
            cancellationId: cancellationID,
            expectedRevision: nil,
            command: command
        )
        let previous = coreCommandTail
        let executor = commandExecutor
        let facade = facade
        coreCommandGeneration &+= 1
        let generation = coreCommandGeneration
        coreCommandTail = Task { @MainActor [weak self] in
            await previous?.value
            guard let self else { return }
            defer {
                if coreCommandGeneration == generation {
                    coreCommandTail = nil
                }
            }
            guard !Task.isCancelled else { return }
            await executor.dispatch(envelope, to: facade)
            guard !Task.isCancelled, drainHostRequests else { return }
            dispatcher.executePendingRequests(from: facade)
        }
    }
}

/// Serializes Rust command execution away from the main actor. The native
/// shell retains ordering while Rust remains the sole durable decision owner.
actor CoreFacadeCommandExecutor {
    func dispatch(_ envelope: CommandEnvelope, to facade: Pod0Facade) {
        facade.dispatch(command: envelope)
    }
}
