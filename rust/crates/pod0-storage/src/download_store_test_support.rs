use pod0_application::{download_input_version, download_intent_id};
use pod0_domain::{CancellationId, CommandId, EpisodeId, StateRevision};

use crate::listening_import_test_support::{ImportFixture, create_legacy_json};
use crate::{
    DownloadEnsureInput, DownloadEnsureOutcome, DownloadWorkflowRecord, LegacyDownloadCutoverInput,
    LibraryStore, StoredDownloadOrigin, commit_listening_cutover,
};

pub(crate) struct DownloadFixture {
    pub(crate) import: ImportFixture,
    pub(crate) store: LibraryStore,
    pub(crate) episode_id: EpisodeId,
}

impl DownloadFixture {
    pub(crate) fn new() -> Self {
        let fixture = Self::new_before_download_cutover();
        fixture
            .store
            .stage_legacy_download_cutover(LegacyDownloadCutoverInput {
                source_generation: 1,
                entries: Vec::new(),
                issued_revision: StateRevision::INITIAL,
                now_ms: 1_800_000_000_002,
                deadline_at_ms: 1_800_000_060_002,
            })
            .unwrap();
        fixture
            .store
            .commit_legacy_download_cutover(1, 1_800_000_000_003)
            .unwrap();
        fixture
    }

    pub(crate) fn new_before_download_cutover() -> Self {
        let import = ImportFixture::new();
        create_legacy_json(&import.source);
        let plan = import.plan();
        import.stage(&plan).unwrap();
        commit_listening_cutover(&import.target, 1_800_000_000_001).unwrap();
        let store = LibraryStore::open_authoritative(&import.target).unwrap();
        let episode_id = store.snapshot().unwrap().episodes[0].episode_id;
        Self {
            import,
            store,
            episode_id,
        }
    }

    pub(crate) fn ensure(&self, command: u64, admitted: bool) -> DownloadWorkflowRecord {
        let snapshot = self.store.snapshot().unwrap();
        let episode = snapshot
            .episodes
            .iter()
            .find(|episode| episode.episode_id == self.episode_id)
            .unwrap();
        let input_version = download_input_version(
            &episode.enclosure_url,
            episode.enclosure_mime_type.as_deref(),
            episode.duration_milliseconds,
        )
        .unwrap();
        let intent_id = download_intent_id(self.episode_id, &input_version).unwrap();
        let outcome = self
            .store
            .ensure_download_workflow(DownloadEnsureInput {
                episode_id: self.episode_id,
                intent_id,
                input_version,
                origin: StoredDownloadOrigin::User,
                admitted,
                wait_failure_code: (!admitted).then(|| "network_unknown".to_owned()),
                command_id: CommandId::from_parts(100, command),
                command_fingerprint: format!("{command:064x}"),
                cancellation_id: CancellationId::from_parts(101, command),
                enclosure_url: episode.enclosure_url.clone(),
                issued_revision: StateRevision::new(command),
                now_ms: 1_800_000_000_100 + i64::try_from(command).unwrap(),
                deadline_at_ms: 1_800_086_400_100 + i64::try_from(command).unwrap(),
            })
            .unwrap();
        match outcome {
            DownloadEnsureOutcome::Changed { record, .. }
            | DownloadEnsureOutcome::Existing(record) => record,
        }
    }
}

pub(crate) fn bytes_file(fixture: &DownloadFixture, name: &str, bytes: &[u8]) -> String {
    let path = fixture.import._directory.path().join(name);
    std::fs::write(&path, bytes).unwrap();
    path.to_string_lossy().into_owned()
}
