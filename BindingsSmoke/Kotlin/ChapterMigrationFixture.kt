import uniffi.pod0_domain.CommandId
import uniffi.pod0_facade.*
import java.io.File

fun qualifyChapterMigrationBoundary() {
    val missing = "/definitely-missing-pod0-chapter-source"
    val inspected = inspectLegacyChapterMigration(missing, missing)
    check(inspected.stage == LegacyChapterMigrationStage.BLOCKED)
    check(inspected.failure?.code == LegacyChapterMigrationFailureCode.STORAGE_UNAVAILABLE)
    check(inspected.report == null && inspected.rollbackExport == null)

    val status = readActiveLegacyChapterMigration(missing)
    check(status.stage == LegacyChapterMigrationStage.BLOCKED)
    check(status.failure?.diagnosticCode == "storage_sqlite")

    val rollback = exportLegacyChapterRollback(missing, missing, missing)
    check(rollback.stage == LegacyChapterMigrationStage.BLOCKED)
    check(rollback.rollbackExport == null)
}

fun qualifyEmptyChapterImport(sourceDatabase: File, coreStore: File, root: File) {
    val artifactRoot = File(root, "legacy-chapter-artifacts").apply { mkdirs() }
    val backupRoot = File(root, "chapter-backups")
    val importId = CommandId(0UL, 6UL)
    val inspected = inspectLegacyChapterMigration(
        sourceDatabase.absolutePath,
        artifactRoot.absolutePath,
    )
    check(inspected.stage == LegacyChapterMigrationStage.INSPECTED)
    val plan = checkNotNull(inspected.plan)
    check(plan.evidenceCount == 0u)
    check(plan.canonicalArtifactCount == 0u)
    check(plan.selectedCount == 0u)
    check(plan.blockedCount == 0u)

    val staged = stageLegacyChapterImport(
        sourceDatabase.absolutePath,
        artifactRoot.absolutePath,
        backupRoot.absolutePath,
        coreStore.absolutePath,
        File(root, "core.chapter-schema-backup.sqlite").absolutePath,
        plan,
        importId,
        CommandId(0UL, 2UL),
    )
    check(staged.stage == LegacyChapterMigrationStage.STAGED)
    check(staged.report?.state == LegacyChapterImportState.STAGED)

    val verified = verifyStagedLegacyChapterImport(
        sourceDatabase.absolutePath,
        artifactRoot.absolutePath,
        backupRoot.absolutePath,
        coreStore.absolutePath,
        importId,
    )
    check(verified.stage == LegacyChapterMigrationStage.VERIFIED)
    val verification = checkNotNull(verified.verification)
    check(verification.verifiedEvidenceCount == 0u)
    check(verification.verifiedArtifactCount == 0u)

    val committed = commitStagedLegacyChapterImport(
        sourceDatabase.absolutePath,
        artifactRoot.absolutePath,
        coreStore.absolutePath,
        importId,
    )
    check(committed.stage == LegacyChapterMigrationStage.IMPORTED)
    check(committed.report?.state == LegacyChapterImportState.IMPORTED)
    check(sharedChapterStoreIsAuthoritative(coreStore.absolutePath))
}
