use std::path::PathBuf;

use pod0_live_hosts::{
    ChatResponse, DownloadResponse, HttpEvidence, HttpGetResponse, RedirectEvidence, TokenUsage,
};

const FINAL_URL: &str =
    "https://final-user:final-password@example.invalid/result?api_key=final-secret&view=full";
const REDIRECT_LOCATION: &str =
    "https://redirect-user:redirect-password@example.invalid/next?token=redirect-secret&step=one";
const REDIRECT_RESOLVED_URL: &str = "https://resolved-user:resolved-password@example.invalid/next?signature=resolved-secret&step=one";

#[test]
fn http_get_response_debug_redacts_url_credentials_without_changing_evidence() {
    let response = HttpGetResponse {
        evidence: credentialed_evidence(),
        body: b"body".to_vec(),
    };

    assert_redacted_debug(&response);
    assert_exact_urls(&response.evidence);
}

#[test]
fn download_response_debug_redacts_url_credentials_without_changing_evidence() {
    let response = DownloadResponse {
        staged_path: PathBuf::from("episode.mp3"),
        byte_count: 4,
        evidence: credentialed_evidence(),
    };

    assert_redacted_debug(&response);
    assert_exact_urls(&response.evidence);
}

#[test]
fn chat_response_debug_redacts_url_credentials_without_changing_evidence() {
    let response = ChatResponse {
        content: Some("done".to_owned()),
        refusal: None,
        tool_calls: Vec::new(),
        usage: TokenUsage::default(),
        response_id: None,
        model: None,
        finish_reason: None,
        created_at: None,
        evidence: credentialed_evidence(),
    };

    assert_redacted_debug(&response);
    assert_exact_urls(&response.evidence);
}

fn credentialed_evidence() -> HttpEvidence {
    HttpEvidence {
        status: 200,
        final_url: FINAL_URL.to_owned(),
        redirects: vec![RedirectEvidence {
            status: 302,
            location: REDIRECT_LOCATION.to_owned(),
            resolved_url: REDIRECT_RESOLVED_URL.to_owned(),
        }],
        entity_tag: None,
        last_modified: None,
        content_type: None,
        content_length: None,
    }
}

fn assert_redacted_debug(value: &impl std::fmt::Debug) {
    let output = format!("{value:?}");
    for secret in [
        "final-user",
        "final-password",
        "final-secret",
        "redirect-user",
        "redirect-password",
        "redirect-secret",
        "resolved-user",
        "resolved-password",
        "resolved-secret",
    ] {
        assert!(!output.contains(secret), "Debug exposed {secret}: {output}");
    }
    assert!(output.contains("view=full"));
    assert!(output.contains("step=one"));
    assert!(output.contains("REDACTED"));
}

fn assert_exact_urls(evidence: &HttpEvidence) {
    assert_eq!(evidence.final_url, FINAL_URL);
    assert_eq!(evidence.redirects[0].location, REDIRECT_LOCATION);
    assert_eq!(evidence.redirects[0].resolved_url, REDIRECT_RESOLVED_URL);
}
