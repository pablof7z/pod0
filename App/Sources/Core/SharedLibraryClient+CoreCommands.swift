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
            await nmp.publishPending(from: facade)
            dispatcher.executePendingRequests(from: facade)
        }
    }

    func coreSnapshot(_ request: ProjectionRequest) async -> ProjectionEnvelope {
        await commandExecutor.snapshot(request, from: facade)
    }

    func executeWorkflowAction(
        _ command: ApplicationCommand,
        action: WorkflowJobAction
    ) async -> WorkflowJobActionResult {
        do {
            _ = try await execute(command)
            return .accepted(action)
        } catch SharedLibraryError.revisionConflict {
            return .stale
        } catch SharedLibraryError.notFound {
            return .notFound
        } catch {
            return .failed
        }
    }
}

/// Serializes Rust command execution away from the main actor. The native
/// shell retains ordering while Rust remains the sole durable decision owner.
actor CoreFacadeCommandExecutor {
    func makeSubscriptions(
        facade: Pod0Facade,
        subscriber: any ProjectionSubscriber
    ) -> SharedLibrarySubscriptions {
        func subscribe(_ scope: ProjectionScope, maxItems: UInt16 = 200) -> SubscriptionId {
            facade.subscribe(
                request: ProjectionRequest(scope: scope, offset: 0, maxItems: maxItems),
                subscriber: subscriber
            )
        }
        return SharedLibrarySubscriptions(
            library: subscribe(.library),
            playback: subscribe(.playback),
            recallConfiguration: subscribe(.recallConfiguration, maxItems: 1),
            chapterWorkflows: subscribe(.chapterWorkflows(episodeId: nil)),
            notes: subscribe(.notes(scope: .all)),
            memories: subscribe(.memories(scope: .all)),
            clips: subscribe(.clips(scope: .active)),
            downloads: subscribe(.downloads(episodeId: nil)),
            transcriptWorkflows: subscribe(.transcriptWorkflows(episodeId: nil)),
            notificationSettings: subscribe(.newEpisodeNotificationSettings, maxItems: 1),
            scheduledAgents: subscribe(.scheduledAgent(taskId: nil))
        )
    }

    func dispatch(_ envelope: CommandEnvelope, to facade: Pod0Facade) {
        facade.dispatch(command: envelope)
    }

    func dispatchThenSubscribe(
        request: ProjectionRequest,
        subscriber: any ProjectionSubscriber,
        envelope: CommandEnvelope,
        to facade: Pod0Facade
    ) -> SubscriptionId {
        facade.dispatch(command: envelope)
        let subscriptionID = facade.subscribe(
            request: request,
            subscriber: subscriber
        )
        return subscriptionID
    }

    func snapshot(
        _ request: ProjectionRequest,
        from facade: Pod0Facade
    ) -> ProjectionEnvelope {
        facade.snapshot(request: request)
    }

    func subscribe(
        _ request: ProjectionRequest,
        subscriber: any ProjectionSubscriber,
        to facade: Pod0Facade
    ) -> SubscriptionId {
        facade.subscribe(request: request, subscriber: subscriber)
    }

    func unsubscribe(_ subscriptionID: SubscriptionId, from facade: Pod0Facade) {
        facade.unsubscribe(subscriptionId: subscriptionID)
    }

    func unsubscribe(
        _ subscriptionIDs: [SubscriptionId],
        from facade: Pod0Facade
    ) {
        for subscriptionID in subscriptionIDs {
            facade.unsubscribe(subscriptionId: subscriptionID)
        }
    }
}
