struct InterruptChapterStep;

impl MigrationObserver for InterruptChapterStep {
    fn reached(&self, boundary: MigrationBoundary) -> Result<(), StorageError> {
        if boundary == (MigrationBoundary::BeforeStepCommit { target: 13 }) {
            Err(StorageError::Interrupted)
        } else {
            Ok(())
        }
    }
}
