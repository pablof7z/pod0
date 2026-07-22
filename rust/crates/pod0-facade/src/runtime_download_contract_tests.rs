use crate::runtime_command_fingerprint::command_fingerprint;
use crate::*;

fn envelope(command_id: u64, command: ApplicationCommand) -> CommandEnvelope {
    CommandEnvelope {
        command_id: CommandId::from_parts(0, command_id),
        cancellation_id: CancellationId::from_parts(0, command_id + 100),
        expected_revision: None,
        command,
    }
}

fn download_request() -> ProjectionRequest {
    ProjectionRequest {
        scope: ProjectionScope::Downloads { episode_id: None },
        offset: 0,
        max_items: 20,
    }
}

#[test]
fn download_contract_is_typed_but_truthfully_unavailable_before_storage_slice() {
    let facade = Pod0Facade::new();
    facade.dispatch(envelope(
        1,
        ApplicationCommand::RequestEpisodeDownload {
            episode_id: EpisodeId::from_parts(0, 9),
            origin: DownloadIntentOrigin::User,
        },
    ));

    let snapshot = facade.snapshot(download_request());
    assert_eq!(snapshot.contract_version, 37);
    let Projection::Downloads { value } = snapshot.projection else {
        panic!("expected download projection");
    };
    assert!(value.workflows.is_empty());
    assert_eq!(
        value.failure.map(|failure| failure.code),
        Some(CoreFailureCode::StorageUnavailable)
    );

    let Projection::Library { value } = facade
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::Library,
            offset: 0,
            max_items: 20,
        })
        .projection
    else {
        panic!("expected library projection");
    };
    assert!(value.operations.iter().any(|operation| {
        operation.command_id == CommandId::from_parts(0, 1)
            && operation.stage == OperationStage::Failed
            && operation.failure.as_ref().map(|failure| failure.code)
                == Some(CoreFailureCode::StorageUnavailable)
    }));
}

#[test]
fn download_command_fingerprints_cover_origin_revision_and_environment() {
    let episode_id = EpisodeId::from_parts(3, 4);
    let user = ApplicationCommand::RequestEpisodeDownload {
        episode_id,
        origin: DownloadIntentOrigin::User,
    };
    let playback = ApplicationCommand::RequestEpisodeDownload {
        episode_id,
        origin: DownloadIntentOrigin::Playback,
    };
    assert_eq!(command_fingerprint(&user), command_fingerprint(&user));
    assert_ne!(command_fingerprint(&user), command_fingerprint(&playback));
    assert_ne!(
        command_fingerprint(&user),
        command_fingerprint(&ApplicationCommand::RequestEpisodeDownload {
            episode_id: EpisodeId::from_parts(3, 5),
            origin: DownloadIntentOrigin::User,
        })
    );

    let cancel_1 = ApplicationCommand::CancelEpisodeDownload {
        episode_id,
        expected_workflow_revision: StateRevision::new(1),
    };
    let cancel_2 = ApplicationCommand::CancelEpisodeDownload {
        episode_id,
        expected_workflow_revision: StateRevision::new(2),
    };
    assert_ne!(
        command_fingerprint(&cancel_1),
        command_fingerprint(&cancel_2)
    );
    let remove_1 = ApplicationCommand::RemoveEpisodeDownload {
        episode_id,
        expected_workflow_revision: StateRevision::new(1),
    };
    let remove_2 = ApplicationCommand::RemoveEpisodeDownload {
        episode_id,
        expected_workflow_revision: StateRevision::new(2),
    };
    assert_ne!(
        command_fingerprint(&remove_1),
        command_fingerprint(&remove_2)
    );
    assert_ne!(
        command_fingerprint(&cancel_1),
        command_fingerprint(&remove_1)
    );

    let wifi = ApplicationCommand::ObserveDownloadEnvironment {
        observation: DownloadEnvironmentObservation {
            network: DownloadNetworkState::Wifi,
            available_capacity_bytes: Some(1_000),
        },
    };
    let cellular = ApplicationCommand::ObserveDownloadEnvironment {
        observation: DownloadEnvironmentObservation {
            network: DownloadNetworkState::Other,
            available_capacity_bytes: Some(1_000),
        },
    };
    assert_ne!(command_fingerprint(&wifi), command_fingerprint(&cellular));
    assert_ne!(
        command_fingerprint(&wifi),
        command_fingerprint(&ApplicationCommand::ObserveDownloadEnvironment {
            observation: DownloadEnvironmentObservation {
                network: DownloadNetworkState::Wifi,
                available_capacity_bytes: Some(1_001),
            },
        })
    );
}

#[test]
fn facade_exports_stable_download_identity_helpers() {
    let input = download_input_version(
        "https://example.test/audio.mp3",
        Some("audio/mpeg"),
        Some(120_000),
    )
    .unwrap();
    let intent = download_intent_id(EpisodeId::from_parts(5, 6), &input).unwrap();
    let first = download_attempt_id(intent, 1).unwrap();
    let second = download_attempt_id(intent, 2).unwrap();

    assert_ne!(first, second);
    assert_eq!(download_attempt_id(intent, 1), Some(first));
}
