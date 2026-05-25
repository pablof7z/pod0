//! Refresh policy + action types. Ports the pure decision-logic portions
//! of `SubscriptionRefreshService.swift`. The legacy service mixed HTTP
//! orchestration with policy; HTTP fetching now lives in
//! `nmp.http.capability` (M5), leaving this crate with the schedule
//! decisions and the etag/last-modified cache shape.

pub mod actions;
pub mod policy;

pub use actions::{
    ExportOpmlAction, ImportOpmlAction, RefreshAllFeedsAction, RefreshFeedAction,
};
pub use policy::{should_refresh, EtagCache, RefreshPolicy};
