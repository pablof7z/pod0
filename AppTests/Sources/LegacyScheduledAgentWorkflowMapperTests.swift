import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

final class LegacyScheduledAgentWorkflowMapperTests: XCTestCase {
    func testMapsPendingRunningBlockedAndSucceededWithoutDualAuthority() throws {
        let base = LegacyScheduledAgentWorkflowTestSupport.baseDate
        let dates = (0..<4).map { base.addingTimeInterval(Double($0 * 300)) }
        let backup = LegacyScheduledAgentWorkflowTestSupport.backup(
            jobs: [
                LegacyScheduledAgentWorkflowTestSupport.job(
                    scheduledFor: dates[0], state: .pending, attempt: 0
                ),
                LegacyScheduledAgentWorkflowTestSupport.job(
                    scheduledFor: dates[1], state: .running, attempt: 1
                ),
                LegacyScheduledAgentWorkflowTestSupport.job(
                    scheduledFor: dates[2], state: .blocked, attempt: 1
                ),
                LegacyScheduledAgentWorkflowTestSupport.job(
                    scheduledFor: dates[3], state: .succeeded, attempt: 1
                ),
            ],
            conversations: [LegacyScheduledAgentWorkflowTestSupport.conversation(
                scheduledFor: dates[3], output: "Qualified legacy output"
            )]
        )

        let mapped = try LegacyScheduledAgentWorkflowMapper.map(backup)
        XCTAssertEqual(mapped.tasks.count, 1)
        XCTAssertEqual(mapped.occurrences.count, 4)
        guard case .pending = mapped.occurrences[0].disposition else {
            return XCTFail("Expected pending")
        }
        guard case .ambiguous(let attempt, _) = mapped.occurrences[1].disposition else {
            return XCTFail("Expected ambiguous")
        }
        XCTAssertEqual(attempt, 1)
        guard case .blocked(let blockedAttempt, let code, _, let retryable)
                = mapped.occurrences[2].disposition else {
            return XCTFail("Expected blocked")
        }
        XCTAssertEqual(blockedAttempt, 1)
        XCTAssertEqual(code, .missingCredential)
        XCTAssertTrue(retryable)
        guard case .succeeded(let succeededAttempt, let output)
                = mapped.occurrences[3].disposition else {
            return XCTFail("Expected succeeded")
        }
        XCTAssertEqual(succeededAttempt, 1)
        XCTAssertEqual(output, "Qualified legacy output")
    }

    func testCompletedCurrentOccurrenceAdvancesImportedRecurrenceOnce() throws {
        let due = LegacyScheduledAgentWorkflowTestSupport.baseDate
        let job = LegacyScheduledAgentWorkflowTestSupport.job(
            scheduledFor: due,
            state: .succeeded,
            attempt: 1
        )
        let backup = LegacyScheduledAgentWorkflowTestSupport.backup(
            jobs: [job],
            conversations: [LegacyScheduledAgentWorkflowTestSupport.conversation(
                scheduledFor: due,
                output: "Finished"
            )]
        )
        let mapped = try LegacyScheduledAgentWorkflowMapper.map(backup)
        let task = try XCTUnwrap(mapped.tasks.first)
        XCTAssertEqual(task.lastRunAt?.date, job.updatedAt)
        XCTAssertEqual(task.nextRunAt.date, job.updatedAt.addingTimeInterval(3_600))
    }

    func testMissingSucceededOutputBecomesRetryableBlockedEvidence() throws {
        let backup = LegacyScheduledAgentWorkflowTestSupport.backup(jobs: [
            LegacyScheduledAgentWorkflowTestSupport.job(
                scheduledFor: LegacyScheduledAgentWorkflowTestSupport.baseDate,
                state: .succeeded,
                attempt: 1
            ),
        ])
        let mapped = try LegacyScheduledAgentWorkflowMapper.map(backup)
        guard case .blocked(_, let code, _, let retryable)
                = mapped.occurrences[0].disposition else {
            return XCTFail("Expected blocked missing output")
        }
        XCTAssertEqual(code, .invalidOutput)
        XCTAssertTrue(retryable)
    }

    func testDuplicateAndMalformedOccurrencesFailClosed() {
        let date = LegacyScheduledAgentWorkflowTestSupport.baseDate
        let row = LegacyScheduledAgentWorkflowTestSupport.job(
            scheduledFor: date,
            state: .pending,
            attempt: 0
        )
        XCTAssertThrowsError(try LegacyScheduledAgentWorkflowMapper.map(
            LegacyScheduledAgentWorkflowTestSupport.backup(jobs: [row, row])
        ))
        let malformed = LegacyScheduledAgentWorkflowTestSupport.job(
            scheduledFor: date,
            state: .pending,
            attempt: 0,
            payloadOverride: nil,
            usePayloadOverride: true
        )
        XCTAssertThrowsError(try LegacyScheduledAgentWorkflowMapper.map(
            LegacyScheduledAgentWorkflowTestSupport.backup(jobs: [malformed])
        ))
    }

    func testVersionedBackupIsImmutableAndDigestVerified() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "scheduled-agent-backup-\(UUID().uuidString)",
            isDirectory: true
        )
        defer { try? FileManager.default.removeItem(at: root) }
        let backup = LegacyScheduledAgentWorkflowTestSupport.backup(jobs: [])
        let evidence = try backup.evidence()
        _ = try backup.publish(to: root, sourceGeneration: 42)
        XCTAssertEqual(
            try LegacyScheduledAgentWorkflowBackup.load(
                from: root,
                sourceGeneration: 42,
                expectedDigest: evidence.digest,
                expectedByteCount: evidence.byteCount
            ),
            backup
        )
        let changed = LegacyScheduledAgentWorkflowBackup(
            formatVersion: backup.formatVersion,
            persistenceGeneration: backup.persistenceGeneration,
            defaultModelReference: "openrouter:changed/model",
            tasks: backup.tasks,
            jobs: backup.jobs,
            artifacts: backup.artifacts,
            conversations: backup.conversations
        )
        XCTAssertThrowsError(try changed.publish(to: root, sourceGeneration: 42))
    }
}
