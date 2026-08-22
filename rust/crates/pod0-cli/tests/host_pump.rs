use std::io::{Read as _, Write as _};
use std::net::TcpListener;
use std::time::{Duration, Instant};

use pod0_application::{CoreWakeReason, DurableLifecycleEffectRequest};
use pod0_cli::{CliRequest, HostConfig, Shell};
use pod0_domain::{
    CancellationId, CommandId, HostRequestId, StateRevision, UnixTimestampMilliseconds,
};
use pod0_facade::{
    ApplicationCommand, CommandEnvelope, Pod0Facade, Projection, ProjectionRequest, ProjectionScope,
};

#[test]
fn opening_store_wakes_for_restart_recovered_leased_work() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0_u8; 4096];
        let _ = stream.read(&mut request).unwrap();
        let body = r#"{"choices":[{"message":{"role":"assistant","content":"Recovered"}}]}"#;
        write_response(&mut stream, body);
    });

    let directory = tempfile::tempdir_in(".").unwrap();
    let store = directory.path().join("pod0.sqlite");
    let facade = Pod0Facade::create(store.to_string_lossy().into_owned()).unwrap();
    let command_id = CommandId::from_parts(501, 1);
    facade.dispatch(CommandEnvelope {
        command_id,
        cancellation_id: CancellationId::from_parts(501, 2),
        expected_revision: None,
        command: ApplicationCommand::StartAgentTurn {
            conversation_id: None,
            user_input: "Recover after restart".to_owned(),
            model_reference: "openai:recovery-model".to_owned(),
        },
    });
    pod0_storage::LibraryStore::open_authoritative(&store)
        .unwrap()
        .claim_next_effect(UnixTimestampMilliseconds::new(now_milliseconds()), 1_000)
        .unwrap()
        .expect("simulate a host that exited while holding the lease");
    drop(facade);

    let mut shell = Shell::new(HostConfig::openai_compatible(
        format!("http://{address}/v1"),
        None,
    ))
    .unwrap();
    assert!(shell.handle(open_request(&store)).ok);
    server.join().unwrap();

    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let reopened = Pod0Facade::open(store.to_string_lossy().into_owned()).unwrap();
        let Projection::AgentConversation { value } = reopened
            .snapshot(ProjectionRequest {
                scope: ProjectionScope::AgentConversation {
                    conversation_id: pod0_domain::ConversationId::from_bytes(
                        command_id.into_bytes(),
                    ),
                },
                offset: 0,
                max_items: 10,
            })
            .projection
        else {
            panic!("expected recovered conversation")
        };
        if value.turns[0].messages.len() == 2 {
            assert_eq!(value.turns[0].messages[1].content, "Recovered");
            break;
        }
        assert!(Instant::now() < deadline, "leased work did not recover");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn persistent_pump_wakes_for_scheduled_core_work() {
    let directory = tempfile::tempdir_in(".").unwrap();
    let store = directory.path().join("pod0.sqlite");
    drop(Pod0Facade::create(store.to_string_lossy().into_owned()).unwrap());
    let now = now_milliseconds();
    let wake_at = UnixTimestampMilliseconds::new(now + 150);
    pod0_storage::LibraryStore::open_authoritative(&store)
        .unwrap()
        .authorize_lifecycle_wake(
            DurableLifecycleEffectRequest {
                request_id: HostRequestId::from_parts(502, 1),
                command_id: CommandId::from_parts(502, 2),
                cancellation_id: CancellationId::from_parts(502, 3),
                issued_revision: StateRevision::INITIAL,
                wake_at,
                reason: CoreWakeReason::Unsupported { wire_code: 7 },
                attempt: 1,
            },
            UnixTimestampMilliseconds::new(now),
        )
        .unwrap();

    let mut shell = Shell::new(HostConfig::empty()).unwrap();
    assert!(shell.handle(open_request(&store)).ok);
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let response = shell.handle(host_drain_request());
        let value = serde_json::to_value(response).unwrap();
        let pending = value
            .pointer("/result/pending")
            .and_then(serde_json::Value::as_array)
            .unwrap();
        if pending.is_empty() {
            break;
        }
        assert!(Instant::now() < deadline, "scheduled work did not progress");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn shutdown_wakes_a_pump_waiting_on_future_work() {
    let directory = tempfile::tempdir_in(".").unwrap();
    let store = directory.path().join("pod0.sqlite");
    drop(Pod0Facade::create(store.to_string_lossy().into_owned()).unwrap());
    let now = now_milliseconds();
    pod0_storage::LibraryStore::open_authoritative(&store)
        .unwrap()
        .authorize_lifecycle_wake(
            DurableLifecycleEffectRequest {
                request_id: HostRequestId::from_parts(503, 1),
                command_id: CommandId::from_parts(503, 2),
                cancellation_id: CancellationId::from_parts(503, 3),
                issued_revision: StateRevision::INITIAL,
                wake_at: UnixTimestampMilliseconds::new(now + 60_000),
                reason: CoreWakeReason::Unsupported { wire_code: 8 },
                attempt: 1,
            },
            UnixTimestampMilliseconds::new(now),
        )
        .unwrap();

    let mut shell = Shell::new(HostConfig::empty()).unwrap();
    assert!(shell.handle(open_request(&store)).ok);
    let started = Instant::now();
    drop(shell);
    assert!(started.elapsed() < Duration::from_secs(1));
}

fn open_request(path: &std::path::Path) -> CliRequest {
    serde_json::from_value(serde_json::json!({
        "v": 1,
        "command": "open_store",
        "path": path.to_string_lossy()
    }))
    .unwrap()
}

fn host_drain_request() -> CliRequest {
    serde_json::from_value(serde_json::json!({
        "v": 1,
        "command": "host_drain",
        "limit": 10
    }))
    .unwrap()
}

fn write_response(stream: &mut std::net::TcpStream, body: &str) {
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(response.as_bytes()).unwrap();
}

fn now_milliseconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis()
        .try_into()
        .unwrap()
}
