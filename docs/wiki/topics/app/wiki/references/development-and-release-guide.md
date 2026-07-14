---
title: "Development And Release Guide"
summary: "Build, test, release, and repo-rule guide for agents working in Podcastr."
tags: [operations, release, rules, testflight, tuist]
aliases: [Podcastr development guide, release guide]
sources:
  - raw/notes/2026-05-12-app-system-source-map.md
created: 2026-05-12
updated: 2026-05-12
verified: 2026-05-12
volatility: warm
confidence: high
---

# Development And Release Guide

Podcastr is a Tuist-based SwiftUI app with an app target, widget target, and unit-test target. It also has repo-specific rules that matter for every implementation pass.

## Build Model

- Generate the project with Tuist when project files are missing or stale.
- `Project.swift` defines app name `Podcastr`, bundle ID `io.f7z.podcast`, widget bundle ID `io.f7z.podcast.widget`, App Group `group.com.podcastr.app`, deployment target iOS 26.0, and the main packages.
- The app target includes `App/Sources/**`, `Assets.xcassets`, and `App/Resources/whats-new.json`.
- The widget target reads through the same App Group boundary.

## Verification

Use targeted tests when touching a narrow subsystem. The `AppTests/Sources/` directory is organized by behavior: RSS parsing, podcast search, playback queue, transcript parsing, RAG, wiki verification, agent tools, BYOK/NIP-46, persistence, downloads, settings encoding, OPML import/export, and more.

For UI or runtime changes, build and launch the app on the requested simulator or device. For provider, transcription, RAG, or network behavior, verify the exact provider path being changed instead of assuming another provider's tests cover it.

## Release Rules

Every commit that ships a user-facing iPhone change must add an entry to `App/Resources/whats-new.json`. The entry id is the short SHA of the commit. Skip entries only for purely internal changes.

CI and upload scripts live under `ci_scripts/`. Versioning and upload behavior are documented in `docs/features.md`; archive/upload logic should stay aligned with those scripts rather than being recreated ad hoc.

## Repo Rules

- Do not add serif fonts. Use SF/system font everywhere. For italics, use system italic APIs or SwiftUI `.italic()`.
- Keep files under the 500-line hard limit; prefer splitting before a file approaches that limit.
- In a dirty worktree, inspect the current status and keep staging scoped unless the user explicitly asks for everything.
- Generated docs/wiki updates should update local indexes and activity logs so navigation stays coherent.

## Operational Caveats

Many app systems are cross-cutting. A provider settings change can affect agent chat, embeddings, wiki generation, transcription, voice, briefing, and Perplexity. A playback change can affect mini-player, Now Playing, lock screen, queue, sleep timer, position persistence, and agent context. A transcript change can affect episode detail, RAG, wiki, search, briefings, summaries, and agent answers.

Start with [[codebase-map|Codebase Map]] ([Codebase Map](codebase-map.md)) before editing.

## See Also

- [[app-operating-model|App Operating Model]] ([App Operating Model](../topics/app-operating-model.md))
- [[data-and-integration-flows|Data And Integration Flows]] ([Data And Integration Flows](../concepts/data-and-integration-flows.md))
- [[launch-floor|Launch Floor]] ([Launch Floor](../../../product/wiki/references/launch-floor.md))

## Sources

- [Podcastr App System Source Map](../../raw/notes/2026-05-12-app-system-source-map.md)
