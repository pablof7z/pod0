use super::*;
#[test]
fn show_coordinate_matches_swift_format() {
    let show = NipF4DiscoveryShow {
        pubkey: "abc123".into(),
        title: "X".into(),
        description: String::new(),
        image_url: None,
        language: None,
        author_pubkey: None,
        categories: vec![],
        created_at: 0,
    };
    assert_eq!(show.coordinate(), "10154:abc123");
}
#[test]
fn show_reference_round_trips_through_wire() {
    let r = ShowReference {
        kind: 10154,
        pubkey: "abc".into(),
        d_tag: "podcast:guid:1".into(),
    };
    assert_eq!(r.to_wire(), "10154:abc:podcast:guid:1");
}
#[test]
fn parse_error_renders_human_message() {
    assert_eq!(
        ParseError::WrongKind {
            expected: 10154,
            got: 1,
        }
        .to_string(),
        "wrong event kind: expected 10154, got 1"
    );
    assert_eq!(
        ParseError::MissingTag("title").to_string(),
        "missing required tag `title`"
    );
}
