//! Unit tests for [`crate::categorization_llm`] parsing.
//!
//! These exercise the pure parse/filter path ([`parse_category_array`] and
//! its `extract_json_array` / `filter_taxonomy` helpers) without a live
//! Ollama — the network call is deliberately split out so this is possible.

use super::{extract_json_array, filter_taxonomy, parse_category_array};
use crate::ffi::actions::categorization_module::MAX_CATEGORIES_PER_EPISODE;

#[test]
fn parses_valid_array() {
    let r = parse_category_array(r#"["Technology", "Science"]"#).unwrap();
    assert_eq!(r, vec!["Technology".to_owned(), "Science".to_owned()]);
}

#[test]
fn handles_preamble_and_fences() {
    let s = "Sure! Here are the categories:\n```json\n[\"Business\", \"Finance\"]\n```\nHope that helps!";
    let r = parse_category_array(s).unwrap();
    assert_eq!(r, vec!["Business".to_owned(), "Finance".to_owned()]);
}

#[test]
fn err_on_empty_response() {
    assert!(parse_category_array("").is_err());
}

#[test]
fn err_on_no_array() {
    assert!(parse_category_array("I cannot categorize this episode.").is_err());
}

#[test]
fn err_when_all_off_taxonomy() {
    // Valid JSON array, but none of the labels are in the fixed taxonomy.
    assert!(parse_category_array(r#"["Gardening", "Knitting"]"#).is_err());
}

#[test]
fn filters_off_taxonomy_labels() {
    let r = parse_category_array(r#"["Technology", "Gardening", "Health"]"#).unwrap();
    assert_eq!(r, vec!["Technology".to_owned(), "Health".to_owned()]);
}

#[test]
fn caps_at_max_categories() {
    let r = parse_category_array(
        r#"["Technology", "Science", "Business", "Health", "Politics"]"#,
    )
    .unwrap();
    assert_eq!(r.len(), MAX_CATEGORIES_PER_EPISODE);
    assert_eq!(
        r,
        vec![
            "Technology".to_owned(),
            "Science".to_owned(),
            "Business".to_owned()
        ]
    );
}

#[test]
fn dedups_repeated_labels() {
    let r = parse_category_array(r#"["Comedy", "Comedy", "Culture"]"#).unwrap();
    assert_eq!(r, vec!["Comedy".to_owned(), "Culture".to_owned()]);
}

#[test]
fn filter_trims_whitespace() {
    let r = filter_taxonomy(vec![" Technology ".to_owned(), "History".to_owned()]);
    assert_eq!(r, vec!["Technology".to_owned(), "History".to_owned()]);
}

#[test]
fn filter_is_case_sensitive_title_case() {
    // Lower-case must not match; the taxonomy is exact title-case.
    let r = filter_taxonomy(vec!["technology".to_owned(), "Science".to_owned()]);
    assert_eq!(r, vec!["Science".to_owned()]);
}

#[test]
fn extract_array_with_surrounding_text() {
    let s = "prefix [\"a\", \"b\"] suffix";
    let extracted = extract_json_array(s).unwrap();
    assert_eq!(extracted, r#"["a", "b"]"#);
}

#[test]
fn extract_array_fails_without_brackets() {
    assert!(extract_json_array("no brackets here").is_err());
}
