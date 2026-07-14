# Baseline Podcast Player Features — Table-Stakes Checklist

> The *floor* every respectable podcast app must clear in 2026. Our AI/agent magic (covered in `briefs/ux-*.md`) rides on top. If any item here is missing at launch, reviewers will correctly call us a half-finished tech demo.

## Competitive lineage — who sets the bar

The features below are inherited from a decade of category leaders. Recognize whose implementation is the reference quality bar:

- **Overcast** — gold standard for **Smart Speed** (silence trim) and **Voice Boost** (loudness/EQ for spoken word).
- **Pocket Casts** — the bar for **cross-device sync** (subs, position, played state), **filters / smart playlists**, and the "Up Next" queue.
- **Castro** — the bar for **inbox / triage queue UX**: Inbox → Queue or archive.
- **Snipd** — the bar for **clip creation, AI chapters, and clip sharing**.
- **Apple Podcasts** — the bar for **directory search, CarPlay, iCloud sync, Apple Subscriptions** OAuth, and platform integrations (Siri, Shortcuts, Watch, AirPlay 2).
- **Spotify** — the bar for **video podcasts** and **co-listen / SharePlay-style social**.
- **Castbox** — the bar for **transcripts/CC at catalog scale**.
- **Podverse / Fountain / Breez** — the bar for **Podcasting 2.0** namespace: chapters, transcripts, persons, soundbite, location, value (V4V tipping), boostagrams.
- **Airr** — historical bar for **timestamped quote sharing**.

Goal is **parity**, not victory, on the floor — so the agent layer is the differentiator, not the crutch.

---

## 1. Playback & audio

| Feature | Description | Priority | Phase |
|---|---|---|---|
| Variable speed control | 0.5x → 3.0x in 0.05x increments; per-show default + global default + transient override | must | v1 |
| Smart-speed / silence trim | Detect and shorten silences without pitch artifacts; on/off per show | must | v1 |
| Voice Boost / dynamic range compression | Loudness EQ tuned for spoken word; on/off per show | must | v1 |
| Volume normalization | Replay-gain-style cross-episode level matching | should | v1.1 |
| Skip forward / back | Configurable durations, asymmetric (fwd 30s default, back 15s default) | must | v1 |
| Long-press skip → scrub | Long-press the skip button enters fine scrub | should | v1.1 |
| Sleep timer | Minutes presets (5/10/15/30/45/60), "end of episode", "end of chapter" | must | v1 |
| Shake-to-extend sleep timer | Shake adds 5 min to active sleep timer (Overcast pattern) | nice | v1.1 |
| Chapter support | Render publisher chapters (ID3, MP4, Podcasting 2.0); jump, skip, label | must | v1 |
| AI chapters | Agent-generated chapter list when none in feed (covered in agent briefs, listed for completeness) | must | v1 |
| AirPlay 2 | Full AirPlay 2 multi-room with metadata | must | v1 |
| Bluetooth controls | Headset play/pause/skip/back/next; codec quality preserved | must | v1 |
| Handoff | iPhone ↔ iPad ↔ Mac ↔ Watch handoff of current playing episode | should | v1.1 |
| CarPlay | Native CarPlay scene with library, queue, now-playing (also in ambient brief) | must | v1 |
| Lock-screen / Now Playing / Control Center | Full MPNowPlayingInfoCenter + remote command center | must | v1 |
| Continuous / autoplay-next | Configurable: end of episode → next in queue, next in show, or stop | must | v1 |
| Smart queue / Up Next | User-orderable queue; "play next" / "play later" actions | must | v1 |
| Resume position | Per-episode position memory, persist across reinstalls (via sync) | must | v1 |
| Mark played / unplayed | Manual + automatic on completion threshold (e.g. last 30s) | must | v1 |
| Bookmarks | Timestamped bookmarks per episode with optional note | must | v1 |
| Clip creation | Trim a 5–120s clip, optional caption, share as audio/video card | should | v1.1 |
| Clip share targets | Universal link, iMessage, Twitter/X, Mastodon, Nostr, copy audio | should | v1.1 |
| Background audio | AVAudioSession `.playback` category, mixWithOthers off by default | must | v1 |
| Interruption handling | Calls, Siri, alarms — pause + resume cleanly | must | v1 |

## 2. Subscription & feed

| Feature | Description | Priority | Phase |
|---|---|---|---|
| Add via RSS URL | Paste feed URL; validate; subscribe | must | v1 |
| OPML import | Import subscription list from another app | must | v1 |
| OPML export | Export current subscriptions as OPML | must | v1 |
| iTunes Search API | Apple Podcasts directory search by show / host / topic | must | v1 |
| Podcast Index integration | podcastindex.org as alt directory + Podcasting 2.0 source | must | v1 |
| Podcasting 2.0 namespace | chapters, transcripts, persons, soundbite, location, value (V4V) | should | v1.1 |
| Manual feed refresh | Pull-to-refresh per show and library-wide | must | v1 |
| Scheduled feed refresh | Background fetch (BGAppRefreshTask) every ~1h on Wi-Fi | must | v1 |
| Episode update detection | Detect re-published episodes (GUID + pubDate change) and flag | should | v1.1 |
| Alt feeds | Show may have multiple feeds (e.g. clean vs explicit); pick one | nice | v2 |
| Premium / private feeds | Token-auth feeds (Patreon, Supercast, Substack, Apple Subs) | should | v1.1 |
| Apple Podcasts Subscriptions | Honor StoreKit subscriptions for paid Apple-hosted shows | nice | v2 |

## 3. Episode management

| Feature | Description | Priority | Phase |
|---|---|---|---|
| Download | Manual download per episode, cancel, retry | must | v1 |
| Auto-download policy | Per show: off / latest N / all new; Wi-Fi only toggle | must | v1 |
| Storage management UI | Per-show + per-episode size, total used, free | must | v1 |
| Auto-delete after played | Configurable: never / 24h / 48h / 7d / immediately | must | v1 |
| Episode size cap | Skip auto-download if file > N MB | nice | v1.1 |
| Filter views | Unplayed / In Progress / Downloaded / Starred / Archived | must | v1 |
| Sort views | Newest, oldest, by show, by duration, recently added | must | v1 |
| Bulk mark all played / unplayed | Per show + per filter | must | v1 |
| Played state visualization | Played, partial (with progress ring), unplayed, in-queue badge | must | v1 |
| Star / favorite | Heart/star toggle, surfaces in Starred filter | should | v1 |
| Archive | Hide from inbox without deleting position/state | should | v1.1 |
| Custom playlists | User-curated ordered lists across shows | should | v1.1 |
| Smart playlists | Rule-based (e.g. "All unplayed news shows < 30 min") | nice | v2 |

## 4. Sync & multi-device

| Feature | Description | Priority | Phase |
|---|---|---|---|
| Subscription sync | Sub list across iPhone / iPad / Mac / Watch | must | v1 |
| Playback position sync | Resume mid-episode on a different device within seconds | must | v1 |
| Played-state sync | Mark-as-played propagates immediately | must | v1 |
| Queue sync | Up Next ordering shared across devices | should | v1.1 |
| New-episode badge sync | Badge counts and "new" flags consistent | should | v1.1 |
| Listening history sync | Cross-device history feed | nice | v1.1 |
| Backend choice | iCloud (CloudKit) for v1 — privacy-preserving, free; backend-of-our-own optional later | must | v1 |
| Conflict resolution | Last-writer-wins per field with vector clock for position | must | v1 |

## 5. Accessibility & internationalization

| Feature | Description | Priority | Phase |
|---|---|---|---|
| Dynamic Type | All text scales through Accessibility XXXL | must | v1 |
| VoiceOver | Every control labeled, hinted, ordered; player is fully VO-driveable | must | v1 |
| High contrast | Honor `accessibilityShouldDifferentiateWithoutColor` | must | v1 |
| Reduce motion | Replace cinematic motion with crossfades when set | must | v1 |
| Reduce transparency | Solid backgrounds when Liquid Glass blur is disabled | must | v1 |
| RTL layouts | Full RTL mirroring (Arabic, Hebrew) | should | v1.1 |
| CJK font handling | Proper fallback fonts for Japanese, Chinese, Korean | should | v1.1 |
| Localized UI | English at v1; Spanish, Portuguese, Japanese, German, French at v1.1 | should | v1.1 |
| Captions / transcripts | Render publisher transcripts; AI transcript fallback (in agent brief) | must | v1 |
| Language indicator | Show podcast language tag; warn on locale mismatch | should | v1.1 |
| Translated transcripts | On-device or server translation of foreign transcripts | nice | v2 |

## 6. Notifications

| Feature | Description | Priority | Phase |
|---|---|---|---|
| New-episode push | APNs push when subscribed show publishes | must | v1 |
| Per-show notification toggle | Mute one show without unsubscribing | must | v1 |
| Quiet hours | Suppress notifications in user-defined window | should | v1.1 |
| Download-complete notif | Optional, off by default | nice | v1.1 |
| Agent / proactive notifs | (covered in `ux-14-proactive-agent-notifications.md`) | — | v1.1 |

## 7. Search

| Feature | Description | Priority | Phase |
|---|---|---|---|
| In-library keyword search | Across show titles, episode titles, descriptions | must | v1 |
| In-show search | Episode list of a single show | must | v1 |
| Apple Podcasts directory search | iTunes Search API | must | v1 |
| Podcast Index search | Alt directory | must | v1 |
| Recent / saved searches | Quick re-run of past queries | nice | v1.1 |
| Semantic / agent search | (covered in `ux-07-search-discovery.md`) | — | v1 |

## 8. Sharing & social

| Feature | Description | Priority | Phase |
|---|---|---|---|
| Share episode link | Universal link, opens in any podcast app via OpenPodcast / share sheet | must | v1 |
| Share with timestamp | Deep link starts at chosen position | must | v1 |
| Share clip | Audio + waveform card, video card with subtitle burn-in | should | v1.1 |
| SharePlay co-listen | Listen-along over FaceTime with synced position | nice | v2 |
| Shortcuts / App Intents | "Play subscribed show", "Resume", "Add to Queue", "Start briefing" | must | v1 |
| Siri integration | Voice control via App Intents | must | v1 |
| Widgets | Now-playing, Up Next, Latest Episode, Briefing-of-the-day | should | v1 |
| Live Activity | Now Playing on Lock Screen + Dynamic Island | must | v1 |

## 9. Monetization meta-features

| Feature | Description | Priority | Phase |
|---|---|---|---|
| V4V Lightning tipping | Stream sats per minute via Podcasting 2.0 value tag | nice | v2 |
| Boostagrams | Send sats + message at a timestamp | nice | v2 |
| In-app tip jar | StoreKit consumable IAPs to creators (App Store rules permitting) | nice | v2 |
| Patreon / Supercast premium feeds | OAuth-style token attach to private feed URL | should | v1.1 |
| Apple Podcasts Subscriptions | StoreKit subscription handling for Apple-hosted shows | nice | v2 |

## 10. Privacy & data

| Feature | Description | Priority | Phase |
|---|---|---|---|
| Analytics opt-out | Per-app analytics off by default; clear toggle | must | v1 |
| Listening-data-stays-on-device stance | Documented; transcripts and embeddings local-first | must | v1 |
| Data export | OPML + JSON dump of history, bookmarks, clips | should | v1.1 |
| Data delete | Wipe all local data + iCloud zone reset | must | v1 |
| Per-show cache clear | Free space without unsubscribing | must | v1 |
| Privacy nutrition label | Honest App Store privacy declarations | must | v1 |
| App Tracking Transparency | If we ever request, prompt cleanly; default no-track | must | v1 |

## 11. Settings

| Feature | Description | Priority | Phase |
|---|---|---|---|
| Theme | Light / dark / system; accent color choice | must | v1 |
| Default speed | Global; per-show override | must | v1 |
| Default skip durations | Forward / back independently | must | v1 |
| Auto-download policy defaults | Apply to new subscriptions | must | v1 |
| Storage limits | Cap total downloaded GB | should | v1 |
| Sleep-timer defaults | Default duration, fade-out behavior | should | v1 |
| Restore from backup | Re-pull from iCloud after reinstall | must | v1 |
| Reset all settings | Single tap to defaults | should | v1.1 |
| Diagnostics export | Send anonymized logs for support | nice | v1.1 |

## 12. Misc / platform

| Feature | Description | Priority | Phase |
|---|---|---|---|
| Apple Watch companion | Standalone playback, queue control, downloads, Now Playing | must | v1 |
| iPad multitasking | Full Stage Manager / Split View / Slide Over support | must | v1 |
| Picture-in-Picture | Video podcasts in PiP | must | v1 |
| Video podcast playback | AVPlayer video surface, fullscreen, AirPlay 2 video | must | v1 |
| External display | Mirror / second-screen for video podcasts | nice | v1.1 |
| Mac Catalyst / native Mac | Mac variant for parity | should | v1.1 |
| Spatial Audio | Honor Dolby Atmos / spatial mixes when present | nice | v2 |
| Family Sharing | Share paid subscriptions with family per Apple rules | nice | v2 |

---

## Phase plan

### v1 — Launch (must reach the floor)
Every row marked **v1**: variable speed, smart speed, voice boost, configurable skip, sleep timer, chapter support, AirPlay 2, CarPlay, lock screen / Now Playing / Control Center, smart queue, resume / mark-played, bookmarks, RSS + OPML import/export, iTunes + Podcast Index search, manual + background refresh, downloads + auto-download policy, storage + auto-delete, filter/sort, played-state viz, iCloud sync (subs + position + played), Dynamic Type, VoiceOver, reduce motion/transparency, captions/transcripts, in-library + directory search, share-with-timestamp, App Intents + Siri, Live Activity, Watch companion, iPad multitasking, PiP + video, theme + accent, default speed/skip, auto-download defaults, restore-from-backup, analytics opt-out, data delete, per-show cache clear. Missing any one is a category-disqualifier.

### v1.1 — Fast-follow (within ~6 weeks of launch)
Volume normalization, long-press scrub, shake-to-extend, handoff, episode-update detection, premium/private feeds (Patreon/Supercast), episode size cap, archive, custom playlists, queue/badge/history sync, RTL + CJK + initial localizations, language indicator, quiet hours, download-complete notifs, recent searches, clip creation + share, data export, reset settings, diagnostics export, Mac Catalyst, external display. The "we heard you" pass.

### v2 — After (post-product-market-fit)
Smart playlists, alt feeds, Apple Podcasts Subscriptions OAuth, translated transcripts, SharePlay co-listen, V4V Lightning tipping, boostagrams, in-app tip jar, Spatial Audio, Family Sharing, community / comments. Ship when the agent magic has earned us the right to a community.

---

## Cross-references

- Differentiating UX in `briefs/ux-01-now-playing.md` … `briefs/ux-15-liquid-glass-system.md` rides on top of this baseline.
- AI chapters / transcripts / wikis / briefings: `ux-04`, `ux-08`, `ux-13`.
- CarPlay / Watch / Widgets / Live Activities: `ux-11-ambient-surfaces.md`.
- Voice mode and barge-in: `ux-06-voice-mode.md`.
- Nostr / friend agent: `ux-12-nostr-communication.md`.

If a feature is listed here *and* in a brief, the brief governs the *experience* and this document governs the *requirement to exist*.
