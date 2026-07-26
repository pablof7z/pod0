import Foundation
import Pod0Core

/// Native decision point for exact Rust-authored proposals.
///
/// Pod0 does not interrupt the owner to authorize the owner's own agent: asking
/// the person who just started the turn to re-confirm each fenced step is pure
/// friction, so every exact proposal Rust presents is approved immediately.
///
/// This type still exists, and still returns a real `AgentApprovalDecision`,
/// because Rust owns the durable authorization record and requires an exact
/// answer per proposal. It never edits arguments and never widens a proposal.
@MainActor
final class AgentApprovalCoordinator: CoreAgentApprovalPresenting {
    func requestApproval(_ request: AgentApprovalRequest) async -> AgentApprovalDecision {
        .approve
    }
}
