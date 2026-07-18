# Pod0 product-proof metrics

Status: Predeclared for M1 evidence collection and the M5 Android investment gate.

This dictionary fixes the measurement rules before Pod0 evaluates results. A
later change must be recorded before viewing gate results; thresholds are not
relaxed after the fact. Each manually exported archive is grouped by its random
anonymous install ID. No signal is transmitted automatically.

## Cohorts and confidence

- An **evaluated install** has at least one `appLaunch` signal and an explicit
  consented/manual export.
- An **activated install** records `firstSubscription:created` and
  `playStarted:succeeded` within 24 hours of its first launch.
- **Meaningful listening** is first crossing five played minutes in an episode.
- **Repeat use** means launches on at least two distinct calendar days during
  days 0–7 after activation. Day boundaries use the device's recorded ISO-8601
  timestamps normalized to UTC during analysis.
- Rates report numerator, denominator, point estimate, and two-sided 95% Wilson
  interval. Latency reports the predefined buckets; no exact timings are stored.
- Product rates need at least 50 applicable installs. Playback rates need 300
  attempts, resume needs 100 attempts, and recall needs 50 asks. Results below
  a minimum are **insufficient evidence**, never a pass.

## Gate dictionary

| Measure | Numerator / denominator | Go threshold | Decision use |
| --- | --- | --- | --- |
| Activation | Activated installs / evaluated installs | Point estimate ≥50%, 95% lower bound ≥35% | The first-use loop reaches listening |
| Repeat use | Activated installs with repeat use / activated installs | Point estimate ≥40%, 95% lower bound ≥25% | Users return without prompting |
| Meaningful listening | Activated installs with `meaningfulListening:succeeded` / activated installs | Point estimate ≥60%, 95% lower bound ≥45% | Playback creates actual listening |
| Play reliability | `playStarted:succeeded` / all `playStarted` outcomes | ≥98.5%, 95% lower bound ≥97% | Native playback is dependable |
| Resume reliability | `resumeAttempt:succeeded` / all resume attempts | ≥97%, 95% lower bound ≥93% | Durable position restores correctly |
| Transcript utility | Installs with `transcriptUsed:used` / installs with `transcriptReady:ready` and meaningful listening | Point estimate ≥40%, 95% lower bound ≥25% | Transcript-first listening adds value |
| Grounded recall | `recallGrounded:grounded` / `recallAsked:started` | Point estimate ≥70%, 95% lower bound ≥55% | Recall returns evidence rather than prose alone |
| Citation use | `recallCitationOpened:opened` / grounded recall results | Point estimate ≥30%, 95% lower bound ≥15% | Evidence leads back to a playable moment |
| Retained artifact | Activated installs with `noteCreated` or `clipCreated` / activated installs | Point estimate ≥20%, 95% lower bound ≥10% | Knowledge is worth preserving |
| Agent repeat use | Activated installs with successful agent turns on two distinct days / activated installs | Point estimate ≥20%, 95% lower bound ≥10% | Agent value extends past novelty |
| Recall latency | Grounded/no-evidence results in buckets below five seconds / completed recall results | ≥95% | Recall feels available in the listening loop |
| Session integrity | Sessions without `uncleanTermination` / all app-launch sessions | ≥99%, with ≥500 sessions | Approximate termination/recovery health |
| Data safety | `dataLossEvidence:detected` | Exactly zero, plus migration/restart test evidence | Data loss is a stop condition |

Playback interruption/route recovery, crash-free sessions, launch/projection
performance, and migration safety also require the device qualification and CI
evidence named in [iOS playback qualification](ios-playback-qualification.md).
`uncleanTermination` is a recovery proxy, not a substitute for organizer crash
reports.

## Architecture gate measures

M5 also records repository-derived evidence; these are not client signals:

- 100% of production Swift files classified by the ownership ratchet.
- Zero unlinked temporary-Swift owners and zero direct UI durable-store writes.
- Apple and Android Rust targets plus Swift/Kotlin binding drift checks green.
- The first listening slice has one Rust source of truth, imported legacy data,
  restart tests, and no obsolete Swift authoritative writer.
- No Apple framework types appear in the shared facade.
- At least the listening/library/resume and transcript evidence identities are
  Rust-owned before an Android go. Remaining shared-priority Swift domains must
  have bounded, sequenced migration issues and may cause a hold.

## Privacy and data handling

Allowed fields are schema version, random signal ID, random install ID,
timestamp, typed signal name/outcome, coarse latency bucket, typed error class,
and optional domain revision. Forbidden fields include podcast or episode
titles, feed/media URLs, transcript/search/recall text, notes, clips, chat
content, credentials, Nostr identifiers, file paths, and stable device/account
identifiers.

Signals are capped at 10,000, remain in application support, fail open without
blocking product actions, and are shared only through an explicit system share
sheet. Disabling collection deletes existing signals and rotates the anonymous
install ID. The user can also delete and rotate independently. Tests must prove
deduplication, opt-out, deletion, schema encoding, recovery markers, and that
forbidden sample content cannot enter an export.

## Gate interpretation

- **Go:** all safety/reliability requirements pass; product measures meet their
  threshold with sufficient evidence; architecture requirements pass.
- **Hold:** evidence is underpowered or a bounded failure has a named remediation
  and retest plan. Android product work remains closed.
- **Stop:** data loss, unacceptable listening reliability, privacy-invalid
  evidence, or product results that reject the core value thesis.

Compilation of Kotlin bindings is readiness evidence only and cannot change a
hold or stop into a go.
