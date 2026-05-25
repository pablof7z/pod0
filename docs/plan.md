# Plan

This is the canonical project plan. Detailed implementation plans live under
`docs/plan/` and are linked from this file. Historical plans under `Plans/`
are reference material, not the active planning surface.

## Current Focus

| Work | State | Source |
|---|---|---|
| NMP feature parity — PR 1: subscribe → library snapshot (PR #29 open). | **In progress** | `docs/plan/nmp-feature-parity.md` |
| Pod0 app rename (PR #2 open). | In progress | `docs/BACKLOG.md` |
| Migrate owned podcast Nostr publishing/discovery from NIP-74 to NIP-F4 (PR #2 open). | In progress | `docs/plan/pod0-nostr-publishing.md` |

## Planning Files

- `docs/plan.md` - overarching plan and milestone status.
- `docs/BACKLOG.md` - tactical queue and follow-ups.
- `WIP.md` - active branches/worktrees only.
- `docs/plan/` - detailed implementation plans linked from this file.
  - `docs/plan/nmp-feature-parity.md` — full NMP feature-parity plan: 74 features, PR sequence, guiding principles, exit criteria.
  - `docs/plan/pod0-nostr-publishing.md` — NIP-F4 podcast publishing plan.
- `Plans/NMP_MIGRATION_PLAN.md` + `Plans/nmp-migration/` - NMP migration plan (M0–M13).

## NMP Migration Milestones (M0–M13)

Sequential; each milestone must pass its exit checklist before the next begins.
Reference: `Plans/nmp-migration/milestones/`.

| Milestone | Title | Status |
|---|---|---|
| M0 | Bootstrap (Rust crate + iOS skeleton + capabilities + migration tooling) | **In progress** |
| M1 | Identity & Nostr foundation | Not started — blocked on M0 |
| M2 | Podcast domain (feeds, RSS) | Not started — blocked on M1 |
| M3 | Audio capability | Not started — blocked on M2 |
| M4 | Background download | Not started — blocked on M2 |
| M5 | Transcripts | Not started — blocked on M4 |
| M6 | Knowledge / RAG | Not started — blocked on M5 |
| M7 | Agent | Not started — blocked on M3, M6 |
| M8 | Voice (STT + TTS + barge-in) | Not started — blocked on M5, M7 |
| M9 | Briefings | Not started — blocked on M3, M7, M8 |
| M10 | Peer agents + NIP-74 + Blossom | Not started — blocked on M1, M7 |
| M11 | CarPlay, Widgets, AppIntents, Spotlight, Handoff | Not started — blocked on M3, M4, M9, M10 |
| M12 | Deletion sweep + lint gate | Not started — blocked on M0–M11 |
| M13 | Second-platform proof | Not started — blocked on M12 |

## Pod0 / NIP-F4 Milestones

| Milestone | Exit Criteria | Status |
|---|---|---|
| Pod0 protocol setup | `AGENTS.md`, `WIP.md`, `docs/plan.md`, and `docs/BACKLOG.md` define the NMP-style workflow. | Done |
| Pod0 app rename | User-facing app name reflects Pod0; stable identifiers unchanged. | In progress (PR #2) |
| NIP-F4 publishing | Publishes kind `10154`/`54`/`10064` with per-podcast keys. | In progress (PR #2) |
| NIP-F4 discovery | Discovery reads kind `10154`/`54`. | In progress (PR #2) |
