//! `Given`-time staging, and the single lazy start that consumes it.
//!
//! A `Given` records plain data — which feed URL serves which bytes — and
//! touches neither disk nor facade. The first step that actually needs a
//! running core calls [`PodWorld::ensure_started`], which prepares a fresh
//! authoritative store (see `store`) and opens the real `Pod0Facade` over
//! it. Both are idempotent, so any step may demand a running world without
//! caring which step ran first.

use pod0_bdd::FixtureEpisode;
use pod0_facade::Pod0Facade;

use super::store::prepare_authoritative_store;
use super::{PodWorld, StoreFixture};

/// One staged feed: the podcast title and episode titles the publisher
/// "currently serves" at a URL. Kept as data rather than bytes so a later
/// step can add an episode and the host renders the CURRENT state of the
/// feed at fetch time, exactly as a real publisher would.
pub(super) struct StagedFeed {
    title: String,
    episode_titles: Vec<String>,
    broken: bool,
}

impl StagedFeed {
    /// The bytes the host returns for this feed right now.
    pub(super) fn render(&self) -> Vec<u8> {
        if self.broken {
            return pod0_bdd::not_a_feed();
        }
        let episodes: Vec<FixtureEpisode> = self
            .episode_titles
            .iter()
            .map(|title| FixtureEpisode {
                title: title.clone(),
            })
            .collect();
        pod0_bdd::rss_feed(&self.title, &episodes).into_bytes()
    }
}

impl PodWorld {
    // ---- Given-time staging (no I/O yet) -------------------------------

    /// `Given a podcast <title> publishes its feed at <url> with episodes
    /// <titles>` — pure data; the host renders the RSS at fetch time.
    pub fn stage_feed(&mut self, title: &str, url: &str, episode_titles: &[String]) {
        let previous = self.staged_feeds.insert(
            url.to_owned(),
            StagedFeed {
                title: title.to_owned(),
                episode_titles: episode_titles.to_vec(),
                broken: false,
            },
        );
        assert!(
            previous.is_none(),
            "pod0-bdd: the scenario staged two feeds at {url:?}; give each feed its own URL"
        );
    }

    /// `Given the feed at <url> serves bytes that are not a podcast feed`.
    pub fn stage_broken_feed(&mut self, url: &str) {
        let previous = self.staged_feeds.insert(
            url.to_owned(),
            StagedFeed {
                title: String::new(),
                episode_titles: Vec::new(),
                broken: true,
            },
        );
        assert!(
            previous.is_none(),
            "pod0-bdd: the scenario staged two feeds at {url:?}; give each feed its own URL"
        );
    }

    /// `When the podcast <title> adds the episode <episode> to its feed` —
    /// the publisher-side event a refresh exists to discover.
    pub fn add_staged_episode(&mut self, podcast_title: &str, episode_title: &str) {
        let feed = self
            .staged_feeds
            .values_mut()
            .find(|feed| feed.title == podcast_title)
            .unwrap_or_else(|| {
                panic!(
                    "pod0-bdd: no staged feed belongs to the podcast {podcast_title:?}, \
                     so it cannot add an episode"
                )
            });
        feed.episode_titles.push(episode_title.to_owned());
    }

    // ---- the single lazy start -----------------------------------------

    /// Prepare the store and open the facade, exactly once per scenario.
    pub fn ensure_started(&mut self) {
        if self.facade.is_some() {
            return;
        }
        let directory =
            tempfile::tempdir().expect("pod0-bdd: a scenario temp directory must be creatable");
        let path = prepare_authoritative_store(&directory);
        let facade = Pod0Facade::open(path.to_string_lossy().into_owned())
            .expect("pod0-bdd: the freshly prepared store must open as authoritative");
        self.store = Some(StoreFixture {
            _directory: directory,
            path,
        });
        self.facade = Some(facade);
    }

    /// The next value of the monotonic fixture counter — the single source
    /// of command identities and observation timestamps.
    pub(super) fn next_count(&mut self) -> u64 {
        self.counter += 1;
        self.counter
    }
}
