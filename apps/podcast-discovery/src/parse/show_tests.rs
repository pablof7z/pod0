use super::*;
fn minimal_tags() -> Vec<Vec<String>> {
    vec![vec!["title".into(), "My Show".into()]]
}
#[test]
fn parse_minimal_show_succeeds() {
    let show = parse_show_event(KIND_SHOW, "agent-pk", 1_700_000_000, "", &minimal_tags())
        .expect("parse");
    assert_eq!(show.title, "My Show");
    assert_eq!(show.pubkey, "agent-pk");
    assert_eq!(show.description, ""); // no description tag, no content
    assert!(show.image_url.is_none());
    assert!(show.author_pubkey.is_none());
    assert!(show.categories.is_empty());
    assert_eq!(show.created_at, 1_700_000_000);
}
#[test]
fn parse_full_show_collects_every_field() {
    let tags = vec![
        vec!["title".into(), "Full Show".into()],
        vec!["description".into(), "A great show".into()],
        vec!["image".into(), "https://img.example/cover.jpg".into()],
        vec!["language".into(), "en".into()],
        vec!["p".into(), "agent-pk".into()],
        vec!["t".into(), "Technology".into()],
        vec!["t".into(), "News".into()],
    ];
    let show = parse_show_event(KIND_SHOW, "agent-pk", 100, "", &tags).expect("parse");
    assert_eq!(show.description, "A great show");
    assert_eq!(show.image_url.as_deref(), Some("https://img.example/cover.jpg"));
    assert_eq!(show.language.as_deref(), Some("en"));
    assert_eq!(show.author_pubkey.as_deref(), Some("agent-pk"));
    assert_eq!(show.categories, vec!["Technology".to_string(), "News".into()]);
}
#[test]
fn parse_rejects_wrong_kind() {
    let err = parse_show_event(1, "pk", 0, "", &minimal_tags()).unwrap_err();
    assert!(matches!(
        err,
        ParseError::WrongKind {
            expected: KIND_SHOW,
            got: 1
        }
    ));
}
#[test]
fn parse_falls_back_title_to_content_prefix() {
    let tags = vec![];
    let show =
        parse_show_event(KIND_SHOW, "pk", 0, "Content as title fallback", &tags).expect("parse");
    assert_eq!(show.title, "Content as title fallback");
}
#[test]
fn parse_rejects_when_no_title_and_no_content() {
    let tags = vec![];
    let err = parse_show_event(KIND_SHOW, "pk", 0, "", &tags).unwrap_err();
    assert_eq!(err, ParseError::MissingTag("title"));
}
#[test]
fn show_to_podcast_maps_fields() {
    let show = NipF4DiscoveryShow {
        pubkey: "pk".into(),
        title: "T".into(),
        description: "S".into(),
        image_url: Some("https://img.example/c.png".into()),
        language: Some("en".into()),
        author_pubkey: Some("pk".into()),
        categories: vec!["Tech".into()],
        created_at: 100,
    };
    let p = show_to_podcast(&show);
    assert_eq!(p.title, "T");
    assert_eq!(p.description, "S");
    assert_eq!(p.language.as_deref(), Some("en"));
    assert_eq!(p.categories, vec!["Tech".to_string()]);
    assert_eq!(p.owner_pubkey_hex.as_deref(), Some("pk"));
    assert_eq!(p.nostr_coordinate.as_deref(), Some("10154:pk"));
    assert_eq!(p.image_url.as_ref().map(Url::as_str), Some("https://img.example/c.png"));
}
#[test]
fn show_to_podcast_id_is_stable_per_coordinate() {
    let make = |pubkey: &str| NipF4DiscoveryShow {
        pubkey: pubkey.into(),
        title: "T".into(),
        description: String::new(),
        image_url: None,
        language: None,
        author_pubkey: None,
        categories: vec![],
        created_at: 0,
    };
    let a = show_to_podcast(&make("pk"));
    let b = show_to_podcast(&make("pk"));
    let c = show_to_podcast(&make("other-pk"));
    assert_eq!(a.id, b.id, "same coordinate → same id");
    assert_ne!(a.id, c.id, "different coordinate → different id");
}
