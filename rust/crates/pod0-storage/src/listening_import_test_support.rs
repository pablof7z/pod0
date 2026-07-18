use std::fs;
use std::path::PathBuf;

use pod0_domain::CommandId;
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use tempfile::TempDir;

use crate::{
    LegacyImportPlan, ListeningImportClock, ListeningImportReport, ListeningImporter, StorageError,
    inspect_legacy_listening_source,
};

pub(crate) const PODCAST_ID: &str = "11111111-1111-1111-1111-111111111111";
pub(crate) const EPISODE_ID: &str = "22222222-2222-2222-2222-222222222222";

pub(crate) struct ImportFixture {
    pub(crate) _directory: TempDir,
    pub(crate) source: PathBuf,
    pub(crate) source_backup: PathBuf,
    pub(crate) target: PathBuf,
    pub(crate) target_backup: PathBuf,
}

impl ImportFixture {
    pub(crate) fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        Self {
            source: directory.path().join("swift.sqlite"),
            source_backup: directory.path().join("swift.backup.sqlite"),
            target: directory.path().join("core.sqlite"),
            target_backup: directory.path().join("core.backup.sqlite"),
            _directory: directory,
        }
    }

    pub(crate) fn plan(&self) -> LegacyImportPlan {
        inspect_legacy_listening_source(&self.source).unwrap()
    }

    pub(crate) fn stage(
        &self,
        plan: &LegacyImportPlan,
    ) -> Result<ListeningImportReport, StorageError> {
        ListeningImporter::new(FixedClock).stage(
            &self.source,
            &self.source_backup,
            &self.target,
            &self.target_backup,
            plan,
            id(1),
            id(2),
        )
    }
}

pub(crate) struct FixedClock;
impl ListeningImportClock for FixedClock {
    fn now_milliseconds(&self) -> i64 {
        1_721_322_000_000
    }
}

pub(crate) fn id(value: u64) -> CommandId {
    CommandId::from_parts(0, value)
}

pub(crate) fn current_metadata(generation: u64) -> Value {
    json!({
        "persistenceGeneration": generation,
        "podcasts": [{
            "id": PODCAST_ID, "kind": "rss", "feedURL": "https://EXAMPLE.test/Feed",
            "title": "Example", "author": "Author", "imageURL": "https://example.test/show.png",
            "description": "Show description", "language": "en", "categories": ["Tech", "News"],
            "discoveredAt": "2024-01-01T00:00:00Z", "titleIsPlaceholder": false,
            "lastRefreshedAt": "2024-01-02T00:00:00Z", "etag": "tag", "lastModified": "yesterday"
        }],
        "subscriptions": [{
            "podcastID": PODCAST_ID, "subscribedAt": "2024-01-03T00:00:00Z",
            "autoDownload": {"mode": {"latestN": {"_0": 3}}, "wifiOnly": false},
            "notificationsEnabled": false, "defaultPlaybackRate": 1.5
        }],
        "episodes": [],
        "settings": {"defaultPlaybackRate": 1.25, "autoMarkPlayedAtEnd": false, "autoPlayNext": false},
        "lastPlayedEpisodeID": EPISODE_ID
    })
}

pub(crate) fn episode(id_value: &str, guid: &str) -> Value {
    json!({
        "id": id_value, "podcastID": PODCAST_ID, "guid": guid, "title": "First episode",
        "description": "Episode description", "pubDate": "2024-01-01T00:00:00Z",
        "duration": 120.5, "enclosureURL": "https://example.test/episode.mp3",
        "enclosureMimeType": "audio/mpeg", "imageURL": "https://example.test/episode.png",
        "playbackPosition": 32.25, "played": true, "isStarred": true,
        "downloadState": {"downloaded": {"byteCount": 4096, "localFileURL": "file:///private/episode.mp3"}},
        "transcriptState": {"ready": {"source": "publisher"}}
    })
}

pub(crate) fn create_sqlite_source(path: &std::path::Path, metadata: &Value, episodes: &[Value]) {
    let connection = Connection::open(path).unwrap();
    connection.execute_batch(
        "CREATE TABLE persistence_metadata(key TEXT PRIMARY KEY,value BLOB NOT NULL);\
         CREATE TABLE episodes(id TEXT PRIMARY KEY,subscription_id TEXT NOT NULL,guid TEXT NOT NULL,\
         pub_date REAL NOT NULL,sort_order INTEGER NOT NULL,payload BLOB NOT NULL);\
         CREATE TABLE workflow_sentinel(value TEXT NOT NULL);\
         INSERT INTO workflow_sentinel VALUES('preserve-me');",
    ).unwrap();
    let generation = metadata["persistenceGeneration"].as_u64().unwrap_or(0);
    connection
        .execute(
            "INSERT INTO persistence_metadata VALUES('generation',?1)",
            [generation.to_string()],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO persistence_metadata VALUES('app_state',?1)",
            [serde_json::to_vec(metadata).unwrap()],
        )
        .unwrap();
    for (index, value) in episodes.iter().enumerate() {
        connection
            .execute(
                "INSERT INTO episodes VALUES(?1,?2,?3,?4,?5,?6)",
                params![
                    value["id"].as_str().unwrap(),
                    PODCAST_ID,
                    value["guid"].as_str().unwrap(),
                    1_704_067_200.0_f64,
                    i64::try_from(index).unwrap(),
                    serde_json::to_vec(value).unwrap()
                ],
            )
            .unwrap();
    }
}

pub(crate) fn create_legacy_json(path: &std::path::Path) {
    let value = json!({
        "subscriptions": [{
            "id": PODCAST_ID, "feedURL": "https://legacy.example/feed", "title": "Legacy",
            "author": "Legacy Author", "description": "Legacy description", "language": "en",
            "categories": ["History"], "subscribedAt": "2023-01-01T00:00:00Z",
            "autoDownload": {"mode": {"off": {}}, "wifiOnly": true}, "isAgentGenerated": false
        }],
        "episodes": [{
            "id": EPISODE_ID, "subscriptionID": PODCAST_ID, "guid": "legacy-guid", "title": "Legacy episode",
            "description": "Remember me", "pubDate": "2023-01-02T00:00:00Z", "duration": 60.0,
            "enclosureURL": "https://legacy.example/episode.mp3", "playbackPosition": 12.0,
            "played": false, "isStarred": true, "downloadState": {"notDownloaded": {}},
            "transcriptState": {"none": {}}
        }],
        "settings": {"defaultPlaybackRate": 1.0}, "lastPlayedEpisodeID": EPISODE_ID
    });
    fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
}
