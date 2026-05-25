//! `podcast-feeds` — RSS parsing, Podcasting 2.0 chapters, OPML I/O, and
//! feed refresh policy.
//!
//! Pure parsing and decision logic. HTTP fetching lives in
//! `nmp.http.capability` (M5); this crate accepts bytes/strings and produces
//! domain values from `podcast-core`.

pub mod opml;
pub mod podcasting2;
pub mod refresh;
pub mod rss;

pub use opml::{export_opml, import_opml, OpmlError};
pub use podcasting2::parse_chapters_json;
pub use refresh::{
    should_refresh, EtagCache, ExportOpmlAction, ImportOpmlAction, RefreshAllFeedsAction,
    RefreshFeedAction, RefreshPolicy,
};
pub use rss::{parse_feed, ParseError, ParsedFeed};
