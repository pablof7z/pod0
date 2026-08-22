use pod0_cli::{CliRequest, HostConfig, Shell};
use pod0_facade::Pod0Facade;
use rusqlite::{Connection, params};

#[test]
fn workflow_settings_are_initialized_by_the_user_command_and_reopen() {
    let directory = tempfile::tempdir_in(".").unwrap();
    let store = directory.path().join("pod0.sqlite");
    let mut shell = Shell::new(HostConfig::empty()).unwrap();
    let create: CliRequest = serde_json::from_value(serde_json::json!({
        "v": 1,
        "command": "create_store",
        "path": store.to_string_lossy()
    }))
    .unwrap();
    assert!(shell.handle(create).ok);

    let setting: CliRequest = serde_json::from_value(serde_json::json!({
        "v": 1,
        "command": "settings_set",
        "setting": {
            "name": "workflow",
            "value": {
                "transcript_provider": "apple_speech",
                "eleven_labs_model": "scribe_v1",
                "assembly_ai_model": "best",
                "open_router_model": "openai/whisper-1",
                "auto_publisher_transcripts": true,
                "auto_provider_transcripts": false,
                "chapter_model": "openai/gpt-4o-mini"
            }
        }
    }))
    .unwrap();
    assert!(shell.handle(setting).ok);
    drop(shell);

    let reopened = Pod0Facade::open(store.to_string_lossy().into_owned()).unwrap();
    let configuration = reopened.workflow_configuration().unwrap().unwrap();
    assert_eq!(configuration.value.chapter_model, "openai/gpt-4o-mini");
    assert!(configuration.value.auto_publisher_transcripts);
}

#[test]
fn settings_subscription_pages_expose_authoritative_totals_beyond_two_hundred() {
    let directory = tempfile::tempdir_in(".").unwrap();
    let store = directory.path().join("pod0.sqlite");
    drop(Pod0Facade::create(store.to_string_lossy().into_owned()).unwrap());
    insert_subscriptions(&store, 205);

    let mut shell = Shell::new(HostConfig::empty()).unwrap();
    let open: CliRequest = serde_json::from_value(serde_json::json!({
        "v": 1,
        "command": "open_store",
        "path": store.to_string_lossy()
    }))
    .unwrap();
    assert!(shell.handle(open).ok);

    let first = settings_page(&mut shell, 0, 200);
    assert_eq!(
        first.pointer("/result/value/subscription_total").unwrap(),
        205
    );
    assert_eq!(
        first
            .pointer("/result/value/subscriptions")
            .and_then(serde_json::Value::as_array)
            .unwrap()
            .len(),
        200
    );
    assert_eq!(
        first.pointer("/result/value/subscriptions_has_more"),
        Some(&serde_json::Value::Bool(true))
    );

    let last = settings_page(&mut shell, 200, 10);
    assert_eq!(
        last.pointer("/result/value/subscription_offset").unwrap(),
        200
    );
    assert_eq!(
        last.pointer("/result/value/subscriptions")
            .and_then(serde_json::Value::as_array)
            .unwrap()
            .len(),
        5
    );
    assert_eq!(
        last.pointer("/result/value/subscriptions_has_more"),
        Some(&serde_json::Value::Bool(false))
    );
}

fn settings_page(shell: &mut Shell, offset: u32, limit: u16) -> serde_json::Value {
    let request: CliRequest = serde_json::from_value(serde_json::json!({
        "v": 1,
        "command": "settings_get",
        "offset": offset,
        "limit": limit
    }))
    .unwrap();
    let response = shell.handle(request);
    assert!(response.ok, "{:?}", response.error);
    serde_json::to_value(response).unwrap()
}

fn insert_subscriptions(path: &std::path::Path, count: u128) {
    let mut connection = Connection::open(path).unwrap();
    let source_import_id: Vec<u8> = connection
        .query_row(
            "SELECT import_id FROM pod0_listening_imports ORDER BY rowid LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let transaction = connection.transaction().unwrap();
    for index in 1..=count {
        let podcast_id = index.to_be_bytes();
        transaction
            .execute(
                "INSERT INTO pod0_podcasts(podcast_id,kind_code,kind_wire_code,feed_url,\
                 feed_key_v1,title,author,image_url,description,language,categories_json,\
                 discovered_at_ms,title_is_placeholder,last_refreshed_at_ms,etag,last_modified,\
                 source_import_id,library_visible) VALUES(?1,1,NULL,NULL,NULL,?2,'',NULL,'',NULL,\
                 '[]',1,0,NULL,NULL,NULL,?3,1)",
                params![
                    podcast_id.as_slice(),
                    format!("Podcast {index}"),
                    source_import_id.as_slice()
                ],
            )
            .unwrap();
        transaction
            .execute(
                "INSERT INTO pod0_subscriptions(podcast_id,subscribed_at_ms,auto_download_code,\
                 auto_download_wire_code,auto_download_latest_count,wifi_only,\
                 notifications_enabled,default_playback_rate_permille,source_import_id,\
                 transcript_start_policy_code,transcript_start_policy_wire_code) \
                 VALUES(?1,1,1,NULL,NULL,1,1,NULL,?2,1,NULL)",
                params![podcast_id.as_slice(), source_import_id.as_slice()],
            )
            .unwrap();
    }
    transaction.commit().unwrap();
}
