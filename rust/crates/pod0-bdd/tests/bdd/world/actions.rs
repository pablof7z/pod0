//! `When`-time acts: everything a scenario does TO the running core, in the
//! two roles native code plays at this boundary — the app (dispatching
//! commands, opening and closing live views, relaunching) and the host
//! (accepting bounded capability work and reporting one typed observation
//! back). No act reads state for assertion; that is `observe`'s job.

use pod0_application::{
    ApplicationCommand, CommandEnvelope, HostObservation, HostObservationEnvelope, HostRequest,
    LeasedHostObservationEnvelope,
};
use pod0_domain::{CancellationId, CommandId, UnixTimestampMilliseconds};
use pod0_facade::Pod0Facade;

use super::observe::library_request;
use super::{IssuedSubscribe, PodWorld};

impl PodWorld {
    /// `When the app subscribes to the feed at <url>` — mints a fresh
    /// command identity from the fixture counter and dispatches the typed
    /// command, remembering the envelope verbatim for retries and lookups.
    pub fn subscribe_to_feed(&mut self, url: &str) {
        self.ensure_started();
        let n = self.next_count();
        let envelope = CommandEnvelope {
            command_id: CommandId::from_parts(0xB0D, n),
            cancellation_id: CancellationId::from_parts(0xCA0, n),
            expected_revision: None,
            command: ApplicationCommand::SubscribeToFeed {
                feed_url: url.to_owned(),
            },
        };
        let previous = self.subscribes.insert(
            url.to_owned(),
            IssuedSubscribe {
                envelope: envelope.clone(),
            },
        );
        assert!(
            previous.is_none(),
            "pod0-bdd: the scenario subscribed to {url:?} twice with fresh identities; \
             use the identical-retry step to repeat a command"
        );
        self.facade().dispatch(envelope);
    }

    /// `When the app repeats the identical subscribe command for <url>` —
    /// the byte-identical envelope, which the contract requires to be
    /// idempotent however far state has advanced.
    pub fn repeat_subscribe(&mut self, url: &str) {
        let envelope = self
            .subscribes
            .get(url)
            .unwrap_or_else(|| {
                panic!("pod0-bdd: the scenario never subscribed to {url:?}, so there is no command to repeat")
            })
            .envelope
            .clone();
        self.facade().dispatch(envelope);
    }

    /// `When the host accepts the pending feed fetch for <url>` — drains the
    /// bounded host-work queue like a native host would and files every
    /// feed-fetch it finds; the named URL must be among them.
    pub fn accept_fetch_work(&mut self, url: &str) {
        self.ensure_started();
        for request in self.facade().next_leased_host_requests(16) {
            if let HostRequest::FetchFeed { feed_url, .. } = &request.request.request {
                self.accepted_fetches.insert(feed_url.clone(), request);
            }
        }
        assert!(
            self.accepted_fetches.contains_key(url),
            "pod0-bdd: the core issued no feed fetch for {url:?}; accepted fetches: {:?}",
            self.accepted_fetches.keys().collect::<Vec<_>>()
        );
    }

    /// `When the host completes the feed fetch for <url>` (and the late
    /// variant) — renders the staged feed's CURRENT bytes as one typed
    /// observation, echoing the accepted request's exact identities. The
    /// accepted work is consumed: a host answers each request once.
    pub fn complete_fetch_work(&mut self, url: &str) {
        if !self.accepted_fetches.contains_key(url) {
            self.accept_fetch_work(url);
        }
        let request = self.accepted_fetches.remove(url).expect("just accepted");
        let bytes = self
            .staged_feeds
            .get(url)
            .unwrap_or_else(|| {
                panic!("pod0-bdd: the scenario never staged a feed at {url:?}, so the host has nothing to return")
            })
            .render();
        let n = self.next_count();
        let receipt = self
            .facade()
            .record_leased_host_observation(LeasedHostObservationEnvelope {
                lease: request.lease,
                observation: HostObservationEnvelope {
                    request_id: request.request.request_id,
                    cancellation_id: request.request.cancellation_id,
                    observed_request_revision: request.request.issued_revision,
                    sequence_number: 0,
                    observed_at: UnixTimestampMilliseconds::new(
                        1_800_000_100_000 + i64::try_from(n).expect("fixture counter fits an i64"),
                    ),
                    observation: HostObservation::FeedBytesFetched {
                        bytes,
                        entity_tag: None,
                        last_modified: None,
                        response_url: url.to_owned(),
                        http_status: 200,
                    },
                },
            });
        self.last_receipt = Some(receipt);
    }

    /// `When the app cancels the subscription to <url>` — dispatches the
    /// typed cancel for the subscribe's cancellation identity, then notes
    /// the state revision so a later `Then` can prove nothing advanced.
    pub fn cancel_subscribe(&mut self, url: &str) {
        let target = self
            .subscribes
            .get(url)
            .unwrap_or_else(|| {
                panic!("pod0-bdd: the scenario never subscribed to {url:?}, so there is nothing to cancel")
            })
            .envelope
            .cancellation_id;
        let n = self.next_count();
        self.facade().dispatch(CommandEnvelope {
            command_id: CommandId::from_parts(0xB0D, n),
            cancellation_id: CancellationId::from_parts(0xCA0, n),
            expected_revision: None,
            command: ApplicationCommand::CancelOperation {
                cancellation_id: target,
            },
        });
        self.revision_at_cancel = Some(self.facade().snapshot(library_request()).state_revision);
    }

    /// `When the app turns on new-episode notifications for <podcast>` —
    /// the user-level act: notifications for that show, plus the global
    /// switch that gates all delivery, each as its own typed command.
    pub fn enable_notifications_for(&mut self, podcast_title: &str) {
        let podcast_id = self.podcast_id_by_title(podcast_title);
        self.dispatch_plain(ApplicationCommand::SetSubscriptionNotifications {
            podcast_id,
            enabled: true,
        });
        self.dispatch_plain(ApplicationCommand::SetNewEpisodeNotificationsEnabled {
            enabled: true,
        });
    }

    /// `When the app turns off new-episode notifications` — the global
    /// switch, which must withdraw undelivered announcements.
    pub fn disable_notifications(&mut self) {
        self.dispatch_plain(ApplicationCommand::SetNewEpisodeNotificationsEnabled {
            enabled: false,
        });
    }

    /// `When the app refreshes the podcast <title>`.
    pub fn refresh_podcast(&mut self, podcast_title: &str) {
        let podcast_id = self.podcast_id_by_title(podcast_title);
        self.dispatch_plain(ApplicationCommand::RefreshPodcast { podcast_id });
    }

    /// `When the host accepts the pending announcement of <episode>` —
    /// drains the work queue and files the one new-episode announcement
    /// naming that episode; it must exist.
    pub fn accept_announcement(&mut self, episode_title: &str) {
        let announcement = self
            .drain_all_host_requests()
            .into_iter()
            .find(|request| {
                matches!(
                    &request.request.request,
                    HostRequest::DeliverNewEpisodeNotification {
                        episode_title: title,
                        ..
                    } if title == episode_title
                )
            })
            .unwrap_or_else(|| {
                panic!(
                    "pod0-bdd: the core issued no new-episode announcement for {episode_title:?}"
                )
            });
        self.accepted_announcement = Some(announcement);
    }

    /// Dispatch one typed command under a fresh fixture identity, for acts
    /// whose scenario never needs to name the command again.
    fn dispatch_plain(&mut self, command: ApplicationCommand) {
        self.ensure_started();
        let n = self.next_count();
        self.facade().dispatch(CommandEnvelope {
            command_id: CommandId::from_parts(0xB0D, n),
            cancellation_id: CancellationId::from_parts(0xCA0, n),
            expected_revision: None,
            command,
        });
    }

    /// `When the app is relaunched` — drops the running facade and every
    /// transient handle, then reopens the SAME store the way the platform
    /// shell does at process start. Only what the store made durable can
    /// survive this line.
    pub fn relaunch(&mut self) {
        let path = self
            .store
            .as_ref()
            .expect("pod0-bdd: the facade must have run before the app can relaunch")
            .path
            .clone();
        self.library_watch = None;
        self.accepted_fetches.clear();
        self.accepted_announcement = None;
        self.last_receipt = None;
        self.facade = None;
        self.facade = Some(
            Pod0Facade::open(path.to_string_lossy().into_owned())
                .expect("pod0-bdd: the prepared store must reopen as authoritative"),
        );
    }
}
