//! Streaming RSS parser with iTunes + Podcasting 2.0 namespace support.
//!
//! Ported from `App/Sources/Podcast/RSSParser.swift` and friends. The parser
//! is pure: it accepts XML bytes and a feed URL, and returns a `ParsedFeed`
//! containing channel-level metadata and a list of episodes. Network I/O
//! lives in `nmp.http.capability` (M5).

pub mod accumulator;
pub mod date;
pub mod parser;
mod state;

pub use accumulator::synthesized_guid;
pub use date::parse_rfc2822;
pub use parser::{parse_feed, ParseError, ParsedFeed};
