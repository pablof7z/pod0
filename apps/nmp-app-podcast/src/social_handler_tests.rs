// Unit tests for social_handler. The handler now requires a live
// PodcastHostOpHandler (with a real Tokio runtime and identity slot),
// so integration coverage lives in the headless scenario binary
// (`scenarios/social.rs`). This file is kept as the #[path] target so
// the compiler doesn't complain about an empty module.
