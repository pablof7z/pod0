import Pod0Core

final class SharedAgentConversationSubscriber: ProjectionSubscriber, @unchecked Sendable {
    private let delivery: @Sendable (ProjectionEnvelope) -> Void

    init(delivery: @escaping @Sendable (ProjectionEnvelope) -> Void) {
        self.delivery = delivery
    }

    func receive(projection: ProjectionEnvelope) {
        delivery(projection)
    }
}
