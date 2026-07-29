use crate::{FixtureEpisode, not_a_feed, parse_quoted_list, rss_feed};

#[test]
fn a_quoted_oxford_comma_list_splits_into_its_titles() {
    assert_eq!(
        parse_quoted_list(r#""Pilot", "Second wind", and "Finale""#),
        vec!["Pilot", "Second wind", "Finale"]
    );
}

#[test]
fn a_two_item_quoted_list_splits_without_a_comma() {
    assert_eq!(
        parse_quoted_list(r#""Pilot" and "Second wind""#),
        vec!["Pilot", "Second wind"]
    );
}

#[test]
fn a_single_quoted_title_is_itself() {
    assert_eq!(parse_quoted_list(r#""Pilot""#), vec!["Pilot"]);
}

#[test]
fn an_unterminated_quote_yields_nothing_after_it() {
    assert_eq!(parse_quoted_list(r#""Pilot" and "broken"#), vec!["Pilot"]);
}

#[test]
fn a_staged_feed_is_deterministic_for_the_same_inputs() {
    let episodes = [FixtureEpisode {
        title: "Pilot".to_owned(),
    }];
    assert_eq!(
        rss_feed("Morning Signal", &episodes),
        rss_feed("Morning Signal", &episodes)
    );
}

#[test]
fn every_staged_episode_appears_with_a_distinct_guid() {
    let episodes = [
        FixtureEpisode {
            title: "Pilot".to_owned(),
        },
        FixtureEpisode {
            title: "Second wind".to_owned(),
        },
    ];
    let xml = rss_feed("Morning Signal", &episodes);
    assert!(xml.contains("<guid>bdd-morning-signal-0</guid>"));
    assert!(xml.contains("<guid>bdd-morning-signal-1</guid>"));
    assert!(xml.contains("<title>Second wind</title>"));
}

#[test]
fn titles_are_escaped_rather_than_breaking_the_document() {
    let xml = rss_feed(
        "Q&A <live>",
        &[FixtureEpisode {
            title: "\"quoted\"".to_owned(),
        }],
    );
    assert!(xml.contains("Q&amp;A &lt;live&gt;"));
    assert!(xml.contains("&quot;quoted&quot;"));
}

#[test]
fn the_malformed_fixture_is_not_xml_at_all() {
    assert!(not_a_feed().starts_with(b"<!doctype html>"));
}
