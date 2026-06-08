use super::*;

fn pending(provider: &str) -> ByokPendingAuthorization {
    ByokPendingAuthorization {
        provider: provider.to_owned(),
        authorization_url: "https://byok.f7z.io/authorize".to_owned(),
        redirect_uri: "podcastr://byok".to_owned(),
        client_id: "com.example.Podcast".to_owned(),
        state: "state-1".to_owned(),
        code_verifier: "verifier-1".to_owned(),
    }
}

#[test]
fn authorization_uses_shared_provider_scopes_and_pkce() {
    let auth = make_authorization(ByokAuthorizationIntent {
        providers: vec![
            "openrouter".to_owned(),
            "ollama".to_owned(),
            "openrouter".to_owned(),
        ],
        redirect_uri: "podcastr://byok".to_owned(),
        client_id: "com.example.Podcast".to_owned(),
        app_name: "Podcastr".to_owned(),
    })
    .unwrap();
    let url = Url::parse(&auth.authorization_url).unwrap();
    let query = url
        .query_pairs()
        .map(|(name, value)| (name.into_owned(), value.into_owned()))
        .collect::<std::collections::HashMap<_, _>>();
    assert_eq!(auth.provider, "openrouter,ollama");
    assert_eq!(query.get("scope").unwrap(), "key:openrouter key:ollama");
    assert_eq!(query.get("code_challenge_method").unwrap(), "S256");
    assert!(!auth.state.is_empty());
    assert!(!auth.code_verifier.is_empty());
}

#[test]
fn callback_validation_rejects_wrong_state() {
    let err = authorization_code_from_callback(
        &pending("openrouter"),
        "podcastr://byok?code=abc&state=wrong",
    )
    .unwrap_err();
    assert!(matches!(err, ByokAuthError::StateMismatch));
}

#[test]
fn callback_validation_extracts_code() {
    let code = authorization_code_from_callback(
        &pending("openrouter"),
        "podcastr://byok?code=abc&state=state-1",
    )
    .unwrap();
    assert_eq!(code, "abc");
}

#[test]
fn token_response_filters_requested_multi_provider_tokens() {
    let response = normalize_token_response(
        ByokTokenWireResponse {
            token_type: "raw_api_key".to_owned(),
            provider: None,
            api_key: None,
            key_id: None,
            key_label: None,
            app_name: Some("Podcastr".to_owned()),
            issued_at: Some(1),
            providers: Some(vec![
                ByokProviderToken {
                    provider: "openrouter".to_owned(),
                    api_key: "sk-or".to_owned(),
                    key_id: Some("or".to_owned()),
                    key_label: None,
                },
                ByokProviderToken {
                    provider: "perplexity".to_owned(),
                    api_key: "sk-pplx".to_owned(),
                    key_id: None,
                    key_label: Some("pplx".to_owned()),
                },
            ]),
        },
        "openrouter,ollama",
    )
    .unwrap();
    assert_eq!(response.provider, "openrouter");
    assert_eq!(response.providers.len(), 1);
}

#[test]
fn token_response_accepts_single_provider_shape() {
    let response = normalize_token_response(
        ByokTokenWireResponse {
            token_type: "raw_api_key".to_owned(),
            provider: Some("Ollama".to_owned()),
            api_key: Some("ollama-key".to_owned()),
            key_id: None,
            key_label: Some("cloud".to_owned()),
            app_name: None,
            issued_at: None,
            providers: None,
        },
        "ollama",
    )
    .unwrap();
    assert_eq!(response.provider, "ollama");
    assert_eq!(response.api_key, "ollama-key");
    assert_eq!(response.providers.len(), 1);
}
