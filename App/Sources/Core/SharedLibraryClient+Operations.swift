import Pod0Core

extension SharedLibraryClient {
    func resolveWaiters(_ operations: [OperationProjection]) {
        for operation in operations {
            guard let waiter = waiters.removeValue(forKey: operation.commandId) else { continue }
            switch operation.stage {
            case .succeeded:
                waiter.continuation.resume(returning: operation.result)
            case .failed, .cancelled, .unsupported:
                waiter.continuation.resume(throwing: SharedLibraryError(operation.failure?.code))
            default:
                waiters[operation.commandId] = waiter
            }
        }
    }
}
