//! `podcast.nostr_episodes` — NIP-F4 (`kind:54`) feedless episode fetch via
//! NMP's relay pool, the canonical `EnsureInterest` + `KernelEventObserver`
//! pattern (mirrors `discover_nostr.rs` for `kind:10154`).
//!
//! ## Design
//!
//! When the user subscribes to a feedless NIP-F4 show (no RSS `feed_url`):
//!
//! 1. `handle_subscribe_nostr` calls `push_interest_via_nmp` with a
//!    `LogicalInterest` for `kind:54` filtered by the podcast's
//!    `owner_pubkey_hex`. NMP opens the subscription through its own relay
//!    pool — no iOS WebSocket (D7).
//! 2. Inbound `kind:54` events fire [`NostrEpisodesObserver::on_kernel_event`].
//! 3. The observer parses each event via `parse_kind_54`, maps it to an
//!    [`Episode`] via `episode_to_episode`, and upserts it into the shared
//!    `PodcastStore` under the existing feedless show row — so the existing
//!    snapshot projection / playback / download pipeline picks it up with zero
//!    changes to `ffi/snapshot.rs` or `register.rs`.
//!
//! ## Doctrine
//!
//! * **D0** — `nmp-core` never names podcast nouns. The kernel emits raw
//!   `KernelEvent`s; this module composes the typed episode.
//! * **D6** — the observer fires best-effort: unparseable or unknown-show
//!   events are silently dropped.
//! * **D7** — NMP's relay pool performs the I/O; this module only declares
//!   the interest and parses results.
//! * **Reactive** — no polling. Episodes ride the push frame triggered by
//!   `SnapshotUpdateSignal::bump()` on every successful upsert.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use nmp_planner::interest::{InterestId, InterestLifecycle, InterestScope, LogicalInterest};
use nmp_planner::stable_hash::stable_hash64;
use nmp_core::substrate::{KernelEvent, ViewDependencies};
use nmp_core::KernelEventObserver;

use podcast_discovery::{episode_to_episode, parse_kind_54, KIND_NIP_F4_EPISODE};
use podcast_core::types::podcast::PodcastId;

use crate::nmp_dispatch::push_interest_via_nmp;
use crate::snapshot_signal::SnapshotUpdateSignal;
use crate::store::PodcastStore;
use nmp_ffi::NmpApp;

/// Guards one-time lazy registration of the [`NostrEpisodesObserver`].
///
/// `handle_subscribe_nostr` registers the observer on first call — not in
/// `ffi/register.rs` (which is constrained) — so the observer is wired
/// before any `kind:54` interest is pushed, ensuring no events are dropped.
///
/// The `OnceLock` makes concurrent first calls safe: only one thread wins the
/// `get_or_init`; the loser sees the initialised `()` and moves on.
static OBSERVER_REGISTERED: OnceLock<()> = OnceLock::new();

/// Namespace discriminant for kind:54 episode interests.
const NOSTR_EPISODES_NAMESPACE: &str = "podcast.nostr_episodes";

/// Derive a stable [`InterestId`] for the kind:54 subscription scoped to
/// `author_pubkey`. One live subscription per subscribed feedless show.
fn episode_interest_id(author_pubkey: &str) -> InterestId {
    InterestId(stable_hash64(&format!(
        "{NOSTR_EPISODES_NAMESPACE}:{author_pubkey}"
    )))
}

/// Build a [`LogicalInterest`] for `kind:54` events by the given author.
///
/// `InterestLifecycle::OneShot` fetches up to 200 historical episodes and
/// closes after EOSE — the relay sends the full back-catalogue in one sweep.
/// A new `EnsureInterest` (re-subscribe dispatch) can re-trigger the fetch.
fn episode_interest(author_pubkey: &str) -> LogicalInterest {
    ViewDependencies {
        kinds: vec![KIND_NIP_F4_EPISODE],
        authors: vec![author_pubkey.to_string()],
        limit: Some(200),
        ..Default::default()
    }
    .into_logical_interest(
        episode_interest_id(author_pubkey),
        InterestScope::Global,
        InterestLifecycle::OneShot,
    )
}

/// Open a `kind:54` subscription for `author_pubkey` via NMP's relay pool.
///
/// Idempotent: the kernel deduplicates interests by `InterestId`, so
/// re-subscribing the same pubkey is a no-op.
pub fn subscribe_nostr_episodes(app: *mut NmpApp, author_pubkey: &str) {
    push_interest_via_nmp(app, episode_interest(author_pubkey));
}

/// `podcast.subscribe_nostr` handler.
///
/// 1. Lazily registers [`NostrEpisodesObserver`] on the first call (so the
///    kernel routes `kind:54` events to the observer before the relay interest
///    is opened — ensuring zero events are dropped during the EOSE sweep).
/// 2. Calls [`subscribe_nostr_episodes`] to open the `kind:54` relay interest.
/// 3. Upserts a followed feedless show row in the store (so the podcast
///    appears in the library immediately, before any episodes arrive).
/// 4. Bumps the snapshot rev so the next projection tick reflects the new row.
///
/// Returns `{"ok": true, "status": "subscribed"}` on success.
pub fn handle_subscribe_nostr(
    app: *mut NmpApp,
    store: &Arc<Mutex<PodcastStore>>,
    rev: &Arc<AtomicU64>,
    snapshot_signal: Option<&SnapshotUpdateSignal>,
    author_pubkey_hex: &str,
    show_title: Option<&str>,
) -> serde_json::Value {
    if author_pubkey_hex.is_empty() {
        return serde_json::json!({"ok": false, "error": "author_pubkey_hex is empty"});
    }

    // Register the observer on the first call (lazily, to avoid touching
    // `ffi/register.rs`). `OnceLock::get_or_init` is atomic: concurrent
    // first calls are safe; only one executes the init closure.
    OBSERVER_REGISTERED.get_or_init(|| {
        if !app.is_null() {
            // SAFETY: `app` is valid for the lifetime of the process —
            // `nmp_app_podcast_register` holds it alive. `register_event_observer`
            // takes `&self`; no exclusive alias exists at this call site.
            let app_ref = unsafe { &*app };
            let observer = Arc::new(
                NostrEpisodesObserver::new(store.clone(), rev.clone())
                    .with_snapshot_signal_opt(snapshot_signal.cloned()),
            );
            let _id = app_ref.register_event_observer(observer);
            // The returned id is intentionally dropped: the observer is
            // permanent (alive for the app's lifetime). `nmp_app_free` joins
            // the actor before dropping the observer slot.
        }
    });

    // Open the kind:54 relay interest (idempotent).
    subscribe_nostr_episodes(app, author_pubkey_hex);

    // Upsert a feedless followed show row so the podcast is visible in the
    // library immediately (episode rows arrive asynchronously via the observer).
    let title = show_title.unwrap_or_else(|| {
        // Minimal placeholder — replaced by the kind:10154 observer.
        "Nostr Show"
    });

    // Use a dummy episode to seed the row if needed; `upsert_feedless_episode`
    // creates the podcast row on first call. When the row already exists the
    // upsert is just a persist() call — not harmful.
    //
    // We only need to ensure the show row exists and is followed. We do not
    // insert a placeholder episode here — that would pollute the episode list.
    // Instead, we call the simpler `subscribe_feedless_show` helper.
    match store.lock() {
        Ok(mut s) => s.subscribe_feedless_show(author_pubkey_hex, title),
        Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
    }

    if let Some(signal) = snapshot_signal {
        signal.bump();
    } else {
        rev.fetch_add(1, Ordering::Relaxed);
    }
    serde_json::json!({"ok": true, "status": "subscribed", "author_pubkey_hex": author_pubkey_hex})
}

/// In-process [`KernelEventObserver`] that turns inbound `kind:54` events
/// into episode rows on the shared `PodcastStore`, under the feedless show
/// row keyed by `author_pubkey`.
///
/// Registered once at init (see [`crate::ffi::register`]) against the same
/// `store` and `rev` slots the snapshot projection reads. Fires on the kernel
/// actor thread between relay frames; [`Self::on_kernel_event`] does only
/// cheap, allocation-bounded work (a kind check, a parse, a store upsert).
pub struct NostrEpisodesObserver {
    store: Arc<Mutex<PodcastStore>>,
    rev: Arc<AtomicU64>,
    snapshot_signal: Option<SnapshotUpdateSignal>,
}

impl NostrEpisodesObserver {
    #[must_use]
    pub fn new(store: Arc<Mutex<PodcastStore>>, rev: Arc<AtomicU64>) -> Self {
        Self {
            store,
            rev,
            snapshot_signal: None,
        }
    }

    /// Builder for the optional snapshot signal.
    ///
    /// Used by lazy registration in [`handle_subscribe_nostr`] where the signal
    /// is already `Option<SnapshotUpdateSignal>`. Prefer this over storing a
    /// plain `AtomicU64::fetch_add` fallback on the caller side.
    pub(crate) fn with_snapshot_signal_opt(
        mut self,
        snapshot_signal: Option<SnapshotUpdateSignal>,
    ) -> Self {
        self.snapshot_signal = snapshot_signal;
        self
    }
}

impl KernelEventObserver for NostrEpisodesObserver {
    fn on_kernel_event(&self, event: &KernelEvent) {
        if event.kind != KIND_NIP_F4_EPISODE {
            return;
        }

        // Parse the kind:54 NIP-F4 episode (exact inverse of the builder).
        let nip_f4_ep = match parse_kind_54(
            event.kind,
            &event.id,
            &event.author,
            event.created_at as i64,
            &event.content,
            &event.tags,
        ) {
            Ok(ep) => ep,
            Err(_) => return, // D6 — unparseable event is dropped silently.
        };

        // Resolve the podcast row for this pubkey. We need the PodcastId
        // to build the Episode domain row (it carries a podcast_id FK).
        // If no row exists yet the upsert helper will create one.
        let podcast_id_opt: Option<PodcastId> = match self.store.lock() {
            Ok(s) => s.podcast_id_for_pubkey(&nip_f4_ep.author_pubkey),
            Err(_) => return, // D6 — poisoned store is a silent no-op.
        };

        // Map NipF4Episode → NipF4DiscoveryEpisode → podcast_core::Episode.
        // `parse_episode_event` is the NIP-F4 parser (uses imeta/url tags);
        // we need the NIP-F4 inverse (audio tag). We already have the parsed
        // NipF4Episode, so map it manually to the domain type using
        // `episode_to_episode` from podcast-discovery's parse layer.
        //
        // We construct an NipF4DiscoveryEpisode-compatible view from our NipF4Episode
        // then call episode_to_episode. Alternatively, since NipF4Episode has
        // all the same fields as the final domain Episode (minus the podcast_id),
        // we build the Episode directly here.
        let podcast_id = podcast_id_opt.unwrap_or_else(|| {
            // Feedless show not yet registered — derive a stable PodcastId from
            // the pubkey so both code paths agree on the id. The upsert helper
            // below will create the row with this exact id.
            PodcastId::new(uuid::Uuid::new_v5(
                &uuid::Uuid::NAMESPACE_URL,
                format!("nostr:show:{}", nip_f4_ep.author_pubkey).as_bytes(),
            ))
        });

        // Build the podcast_core::Episode from the NipF4Episode fields.
        let episode = build_episode_from_nip_f4(&nip_f4_ep, podcast_id);

        // Determine show title for the feedless row. Use the podcast's
        // existing title when the row already exists; fall back to a minimal
        // placeholder. A richer title arrives via the kind:10154 observer.
        let show_title = match self.store.lock() {
            Ok(s) => s
                .podcast(podcast_id)
                .map(|p| p.title.clone())
                .unwrap_or_else(|| format!("Nostr Show ({})", &nip_f4_ep.author_pubkey[..8.min(nip_f4_ep.author_pubkey.len())])),
            Err(_) => return,
        };

        let changed = match self.store.lock() {
            Ok(mut s) => {
                s.upsert_feedless_episode(
                    &nip_f4_ep.author_pubkey,
                    &show_title,
                    episode,
                );
                true
            }
            Err(_) => false,
        };

        if changed {
            if let Some(signal) = &self.snapshot_signal {
                signal.bump();
            } else {
                self.rev.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

/// Build a [`podcast_core::Episode`] from a parsed [`podcast_discovery::NipF4Episode`].
///
/// Uses `podcast_discovery::episode_to_episode` by constructing a bridge
/// `NipF4DiscoveryEpisode`. The `d_tag` is the event id (stable per event, serves as
/// the episode GUID for dedup).
fn build_episode_from_nip_f4(
    ep: &podcast_discovery::NipF4Episode,
    podcast_id: PodcastId,
) -> podcast_core::Episode {
    use podcast_discovery::NipF4DiscoveryEpisode;

    let discovery_ep = NipF4DiscoveryEpisode {
        d_tag: ep.event_id.clone(),
        title: ep.title.clone(),
        summary: ep.description.clone().unwrap_or_default(),
        published_at: ep.created_at,
        duration_secs: ep.duration_secs,
        image_url: ep.image_url.clone(),
        audio_url: ep.audio_url.clone(),
        audio_mime_type: ep.audio_mime_type.clone(),
        audio_sha256_hex: None,
        audio_size_bytes: None,
        show_a_tag: None,
        chapters_url: ep.chapters_url.clone(),
        transcript_url: ep.transcript_url.clone(),
        transcript_mime_type: ep.transcript_mime_type.clone(),
    };
    episode_to_episode(&discovery_ep, podcast_id)
}

#[cfg(test)]
#[path = "nostr_episodes_tests.rs"]
mod tests;
