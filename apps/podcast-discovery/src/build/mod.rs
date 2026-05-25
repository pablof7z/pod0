//! Build NIP-74 event tag sets from `podcast_core` domain rows.
//!
//! The functions here only construct the `Vec<Vec<String>>` tag payload
//! (and an event-content string when relevant). Signing and relay
//! publishing belong to the kernel-side action modules — keeping this
//! module pure means it's trivially testable and can be reused on both
//! the iOS publish path and the eventual NIP-F4 cutover.

mod episode;
mod show;

pub use episode::episode_to_episode_tags;
pub use show::{podcast_to_show_tags, show_content};
