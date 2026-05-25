//! Podcasting 2.0 namespace helpers. Currently covers JSON chapters fetched
//! via `<podcast:chapters url="…">`. Person/soundbite/transcript metadata
//! is handled inline by the RSS parser.

pub mod chapters;

pub use chapters::{parse_chapters_json, ChaptersError};
