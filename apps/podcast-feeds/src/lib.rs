//! `podcast-feeds` — RSS parsing, Podcasting 2.0 chapters, OPML I/O, and
//! feed refresh policy.
//!
//! Pure parsing and decision logic. HTTP fetching lives in
//! `nmp.http.capability`; this crate accepts bytes/strings and produces
//! domain values from `podcast-core`.
//!
//! M5 added two pieces to this crate:
//!
//! - [`http`] — the Rust mirror of the iOS `HttpCapability` wire vocabulary
//!   (`HttpRequest`, `HttpResult`, `HttpMethod`). These live here (not in
//!   `nmp-app-podcast`) so the FeedClient and any future low-level
//!   consumer can type-check requests without a back-dep on the per-app
//!   crate. `nmp-app-podcast::capability::http` re-exports them.
//! - [`client`] — `FeedClient` orchestration: builds the conditional-GET
//!   request from an [`EtagCache`] and interprets the [`HttpResult`] into
//!   a [`client::FeedResult`] (NotModified vs Parsed).

pub mod client;
pub mod http;
pub mod opml;
pub mod podcasting2;
pub mod refresh;
pub mod rss;

pub use client::{build_feed_request, handle_feed_response, FeedError, FeedResult};
pub use http::{
    HttpCommand, HttpMethod, HttpReport, HttpRequest, HttpResult,
    HTTP_ASYNC_CAPABILITY_NAMESPACE, HTTP_CAPABILITY_NAMESPACE,
};
pub use opml::{export_opml, import_opml, OpmlError};
pub use podcasting2::parse_chapters_json;
pub use refresh::{
    should_refresh, EtagCache, ExportOpmlAction, ImportOpmlAction, RefreshAllFeedsAction,
    RefreshFeedAction, RefreshPolicy,
};
pub use rss::{parse_feed, ParseError, ParsedFeed};
