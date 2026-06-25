# Scenario Index

All scenarios for the Podcastr iOS simulator test suite. See
[`../README.md`](../README.md) for setup, simulator config, and how to record results.

## A — Onboarding & Identity
- **A1** [`a1-onboarding-fresh-install.md`](a1-onboarding-fresh-install.md) — Fresh install: generate new Nostr keypair, walk the 5-page onboarding, enter the app.
- **A2** [`a2-import-nsec.md`](a2-import-nsec.md) — Import an existing `nsec` private key via Advanced → Use my own key.
- **A3** [`a3-profile-setup.md`](a3-profile-setup.md) — Set display name, username, about, and avatar in Edit Profile.
- **A4** [`a4-remote-signer-nip46.md`](a4-remote-signer-nip46.md) — Connect a NIP-46 remote signer (bunker URI / QR pairing).
- **A5** [`a5-account-details-relays.md`](a5-account-details-relays.md) — Inspect account details (npub/hex/fp), copy keys, review relay/networking config.

## B — Podcast Discovery & Search
- **B1** [`b1-search-keyword.md`](b1-search-keyword.md) — Search podcasts by keyword and inspect Shows/Episodes/Transcripts result sections.
- **B2** [`b2-search-rss-url.md`](b2-search-rss-url.md) — Add a show directly by pasting an RSS feed URL.
- **B3** [`b3-search-nostr-naddr.md`](b3-search-nostr-naddr.md) — Discover/subscribe a Nostr-published podcast (NIP-F4 kind:10154) via the Nostr tab.
- **B4** [`b4-browse-trending-recommended.md`](b4-browse-trending-recommended.md) — Browse Home recommended/inbox/agent-pick sections and trending in Add Show.
- **B5** [`b5-subscribe-podcast.md`](b5-subscribe-podcast.md) — Subscribe to a podcast and confirm it appears in the library.
- **B6** [`b6-unsubscribe-podcast.md`](b6-unsubscribe-podcast.md) — Unsubscribe (keep history) and verify removal from library.

## C — Library & Episode Management
- **C1** [`c1-view-library.md`](c1-view-library.md) — View subscribed podcasts grid/list in the Library tab.
- **C2** [`c2-browse-episodes.md`](c2-browse-episodes.md) — Open a show, browse its episodes, search within the show.
- **C3** [`c3-download-episode.md`](c3-download-episode.md) — Download an episode and watch the progress → downloaded → remove lifecycle.
- **C4** [`c4-mark-played-unplayed.md`](c4-mark-played-unplayed.md) — Mark an episode played/unplayed via context menu and swipe actions.
- **C5** [`c5-episode-detail.md`](c5-episode-detail.md) — Inspect the episode detail view: hero, categories, chapters, show notes, actions.

## D — Playback
- **D1** [`d1-play-pause-resume.md`](d1-play-pause-resume.md) — Play an episode, pause, resume; verify mini-player and full player.
- **D2** [`d2-skip-forward-back.md`](d2-skip-forward-back.md) — Skip forward/back by the configured interval; long-press for chapter jump.
- **D3** [`d3-seek-scrubber.md`](d3-seek-scrubber.md) — Seek via the scrubber drag and timeline tap.
- **D4** [`d4-playback-speed.md`](d4-playback-speed.md) — Change playback speed via the speed sheet.
- **D5** [`d5-sleep-timer.md`](d5-sleep-timer.md) — Set and clear a sleep timer (including End-of-episode).
- **D6** [`d6-now-playing-lockscreen.md`](d6-now-playing-lockscreen.md) — Verify Now Playing / Control Center / lock-screen transport controls.
- **D7** [`d7-queue-up-next.md`](d7-queue-up-next.md) — Add to queue, reorder (Move to top), remove, clear; auto-play next.
- **D8** [`d8-chapters.md`](d8-chapters.md) — Navigate chapters; tap to seek; ad-overlap chapter flagging.
- **D9** [`d9-autosnip.md`](d9-autosnip.md) — Capture a 30-second AutoSnip clip from the player.
- **D10** [`d10-ad-skip.md`](d10-ad-skip.md) — Pre-roll/ad skip button + auto-skip-ads behavior.

## E — Transcripts
- **E1** [`e1-view-publisher-transcript.md`](e1-view-publisher-transcript.md) — View a publisher-supplied transcript synced to playback.
- **E2** [`e2-whisper-transcription.md`](e2-whisper-transcription.md) — Trigger AI (OpenRouter Whisper) transcription for an episode with no transcript.
- **E3** [`e3-tap-segment-seek.md`](e3-tap-segment-seek.md) — Tap a transcript segment to seek playback to that position.
- **E4** [`e4-search-transcript.md`](e4-search-transcript.md) — Search within a transcript / kernel knowledge search from the Search tab.

## F — NIP-84 Highlights (Clippings)
- **F1** [`f1-create-highlight.md`](f1-create-highlight.md) — Create a clipping/highlight from a transcript segment.
- **F2** [`f2-verify-nip84-metadata.md`](f2-verify-nip84-metadata.md) — Verify NIP-84 metadata (a-tag, alt, context) and that the highlight is contextual, not a random time slice.
- **F3** [`f3-view-clippings.md`](f3-view-clippings.md) — View existing clippings in the Clippings tab (Today/This Week/Earlier buckets).
- **F4** [`f4-share-clipping.md`](f4-share-clipping.md) — Share a clipping.
- **F5** [`f5-delete-clipping.md`](f5-delete-clipping.md) — Delete a clipping via context menu / swipe.

## G — AI Agent Interaction
- **G1** [`g1-configure-ollama.md`](g1-configure-ollama.md) — Configure the Ollama provider + select `deepseek-v4-flash:cloud`.
- **G2** [`g2-open-agent-chat.md`](g2-open-agent-chat.md) — Open the agent chat surface and exercise the composer/history/new-conversation.
- **G3** [`g3-ask-episode-question.md`](g3-ask-episode-question.md) — Ask the agent a question about an episode; verify it uses transcript/knowledge.
- **G4** [`g4-agent-highlight-suggestion.md`](g4-agent-highlight-suggestion.md) — Ask the agent to suggest/create a highlight; verify the `.agent`-sourced clip.
- **G5** [`g5-voice-mode.md`](g5-voice-mode.md) — Exercise voice mode (orb states, talk/stop/switch-to-text).

## H — Social / Nostr Features
- **H1** [`h1-follow-user.md`](h1-follow-user.md) — Follow another Nostr user (friend) by pubkey.
- **H2** [`h2-friend-activity.md`](h2-friend-activity.md) — View a friend's detail / listening activity and add a note.
- **H3** [`h3-share-episode-nostr.md`](h3-share-episode-nostr.md) — Share an episode (deeplink / system share / quote share).
- **H4** [`h4-nipf4-publishing.md`](h4-nipf4-publishing.md) — NIP-F4 publishing path for an owned podcast (author claim kind:10064).
- **H5** [`h5-feedback-comments.md`](h5-feedback-comments.md) — Episode comments + the Feedback compose/thread flow.

## I — Settings
- **I1** [`i1-configure-ai-providers.md`](i1-configure-ai-providers.md) — Configure AI providers (Ollama + model selection).
- **I2** [`i2-configure-openrouter-whisper.md`](i2-configure-openrouter-whisper.md) — Configure OpenRouter and enable Whisper transcription fallback.
- **I3** [`i3-playback-settings.md`](i3-playback-settings.md) — Playback settings: default speed, skip intervals, auto-mark/auto-play/auto-skip.
- **I4** [`i4-storage-downloads-settings.md`](i4-storage-downloads-settings.md) — Storage/downloads management, data export, clear-all-data.
- **I5** [`i5-notification-settings.md`](i5-notification-settings.md) — Notification permission + per-show / new-episode alert toggles.

## J — Edge Cases & Regression
- **J1** [`j1-offline-mode.md`](j1-offline-mode.md) — Offline behavior: play downloaded, search/network failure handling.
- **J2** [`j2-background-playback.md`](j2-background-playback.md) — App backgrounded during playback; audio continues; resume on foreground.
- **J3** [`j3-long-episode.md`](j3-long-episode.md) — Very long episode (>3h): time formatting, seeking, transcript scale.
- **J4** [`j4-no-transcript-episode.md`](j4-no-transcript-episode.md) — Episode with no transcript: generate affordance / graceful empty state.
- **J5** [`j5-network-error-search.md`](j5-network-error-search.md) — Network errors during search / subscribe; error surfaces, no crash.
- **J6** [`j6-resume-after-relaunch.md`](j6-resume-after-relaunch.md) — Resume playback position after killing and relaunching the app.

## K — NIP-84 Highlights, Deep (relay-verified, contextual boundaries)
> Deeper coverage of the app's headline differentiator: LLM-informed, **contextual**
> highlights verified as raw kind:9802 events on `relay.primal.net` with `nak`.
> Note the kernel contract: highlights anchor the source with an **`i`-tag**
> (`podcast:item:guid:<guid>#t=<start>,<end>`), store the excerpt in a **`context`**
> tag, and the caption in an **`alt`** tag — there is **no `a`-tag** (corrects F2).
- **K1** [`k1-pubkey-to-hex-nak-setup.md`](k1-pubkey-to-hex-nak-setup.md) — Resolve hex pubkey via `nak decode` (fixes F2's malformed-pubkey block); prove a `nak req` round-trip.
- **K2** [`k2-create-clip-and-verify-9802-on-relay.md`](k2-create-clip-and-verify-9802-on-relay.md) — Create a clip; verify the raw kind:9802 event's tag set (`i`/`context`/`alt`, no `a`) on the relay.
- **K3** [`k3-clip-composer-sentence-snap-boundaries.md`](k3-clip-composer-sentence-snap-boundaries.md) — Clip composer: boundaries snap to transcript utterances, not a fixed N-second window.
- **K4** [`k4-verify-i-tag-content-vs-transcript.md`](k4-verify-i-tag-content-vs-transcript.md) — Cross-reference the event `content`/`context` against the transcript; verify complete-thought boundaries.
- **K5** [`k5-openrouter-whisper-transcript-pipeline.md`](k5-openrouter-whisper-transcript-pipeline.md) — Full pipeline: OpenRouter Whisper transcript → clip → relay verification.
- **K6** [`k6-autosnip-to-9802-publish.md`](k6-autosnip-to-9802-publish.md) — AutoSnip (D9) → NIP-84 publish → relay verification; ~30s refined to utterance edges.
- **K7** [`k7-9802-tag-negative-checks.md`](k7-9802-tag-negative-checks.md) — Negative/contract checks: no a-tag; agent clips and empty-transcript clips do NOT publish.

## L — AI Agent & Ollama, Deep (live inference, grounded answers)
> Real `deepseek-v4-flash:cloud` inference via Ollama (no stubs): model selection,
> transcript-grounded Q&A, agent-created contextual highlights, conversation
> continuity. Unblocks G1–G4.
- **L1** [`l1-select-deepseek-model-for-agent-role.md`](l1-select-deepseek-model-for-agent-role.md) — Settings → Intelligence → Models: select `deepseek-v4-flash:cloud` for the Agent role (unblocks G1).
- **L2** [`l2-ollama-connect-check-models.md`](l2-ollama-connect-check-models.md) — Ollama: connect (BYOK/manual/endpoint) + "Check Available Models" model count.
- **L3** [`l3-open-agent-chat-and-stub-roundtrip.md`](l3-open-agent-chat-and-stub-roundtrip.md) — Open agent chat from the root toolbar `agent.open` (fixes G2 nav); stub round-trip + history/new.
- **L4** [`l4-grounded-episode-qa-live-ollama.md`](l4-grounded-episode-qa-live-ollama.md) — Transcript-grounded Q&A with live Ollama; concrete questions + negative control (unblocks G3).
- **L5** [`l5-agent-creates-contextual-highlight.md`](l5-agent-creates-contextual-highlight.md) — Agent creates a contextual highlight; tool-batch, Agent badge, idea-aligned boundaries (extends G4).
- **L6** [`l6-agent-conversation-continuity.md`](l6-agent-conversation-continuity.md) — Multi-turn context retention, new-conversation isolation, history resume.
- **L7** [`l7-transcript-search-vs-agent-crossref.md`](l7-transcript-search-vs-agent-crossref.md) — Cross-reference transcript search (E4) with agent answers to catch hallucination.

## M — Voice & End-to-End Capstone
> The canonical kernel voice path and a full-loop smoke test.
- **M1** [`m1-voice-mode-kernel-path.md`](m1-voice-mode-kernel-path.md) — Canonical kernel VoiceView (`podcast.voice`), orb/state machine via the voice projection (fixes G5 surface mix-up).
- **M2** [`m2-voice-note-vs-voiceview-disambiguation.md`](m2-voice-note-vs-voiceview-disambiguation.md) — Disambiguate the Voice note half-sheet vs the kernel VoiceView (root cause of G5's block).
- **M3** [`m3-full-pipeline-capstone.md`](m3-full-pipeline-capstone.md) — Full loop: providers → transcript → agent clip → relay-verified NIP-84, with grounding cross-check.
