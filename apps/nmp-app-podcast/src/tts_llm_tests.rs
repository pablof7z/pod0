//! Tests for [`super::tts_llm`] — pure script-parsing seam coverage.
//!
//! These exercise [`super::parse_script`], the deterministic seam
//! `generate_tts_script` applies to the model reply. They run without a live
//! Ollama: a valid reply yields a non-empty script, and an empty / whitespace
//! reply yields `Err`, which is the condition the call site
//! (`tts::handle_generate`) treats as "fall back to `placeholder_script`".

use super::parse_script;

#[test]
fn parse_script_returns_nonempty_text() {
    let reply = "Welcome to today's episode about Rust ownership. Let's dive in!";
    let script = parse_script(reply).expect("valid reply parses");
    assert!(!script.is_empty());
    assert_eq!(script, reply);
}

#[test]
fn parse_script_trims_surrounding_whitespace() {
    let reply = "\n\n  Here is the script.  \n";
    let script = parse_script(reply).expect("padded reply parses");
    assert_eq!(script, "Here is the script.");
}

#[test]
fn fallback_on_err() {
    // An empty / whitespace-only reply is the failure the call site catches to
    // fall back to `placeholder_script`.
    assert!(parse_script("").is_err());
    assert!(parse_script("   \n\t  ").is_err());
}
