---
title: NMP Version Upgrades
slug: nmp-version-upgrades
topic: project-setup
summary: The changelog path must use a generalized versioned template (nmp-v<version>-upgrade.md) instead of being hardcoded to v0.1.0
tags:
  - capture
volatility: warm
confidence: medium
created: 2026-06-02
updated: 2026-06-13
verified: 2026-06-02
compiled-from: conversation
sources:
  - session:8bfa1b91-b40c-44b3-acb9-245b36f4c841
  - session:c43d5e77-d667-4e71-a574-47aaab5b6a7a
  - session:52b667b5-ed45-479e-a960-1baeefbbdf03
  - session:c33b9adb-9d1a-4717-9314-b45a61e6cbc3
  - session:38f8143c-c90d-49e3-a8fa-8d5ca17ac319
  - session:rollout-2026-05-10T20-46-06-019e12ff-12ba-79d2-a14c-78a7ec6b0bfa
  - session:c1691db0-d63e-4062-adad-1cfa0d679d09
  - session:rollout-2026-05-11T09-10-31-019e15a8-991d-7890-957e-f45fb0ff5a7c
  - session:rollout-2026-05-25T12-53-43-019e5e8d-f919-7521-a540-9ca4b95f10ff
  - session:rollout-2026-05-26T09-10-44-019e62e8-3112-7080-98ca-48ac46d8b8d2
---

# NMP Version Upgrades

## Changelog Path

The changelog path must use a generalized versioned template (nmp-v<version>-upgrade.md) instead of being hardcoded to v0.1.0. <!-- [^8bfa1-2] -->

## Current Version Pin

NMP v0.6.0 (merged as #414) is the pinned version; it provides nmp-blossom (typed upload action with sign+transport, supporting both nsec and NIP-46 bunker) and nmp-nip02 (reactive FollowListProjection + ActiveFollowSet membership predicate), dissolving the two biggest blockers for Blossom kernel upload and social-graph trust gating. The shipped nmp-blossom and nmp-nip02 implementations make the original BACKLOG designs for these net-new capabilities obsolete. The nmp-nip02 dependency was added explicitly to the workspace and crate Cargo.toml per D11 (single-door deps), at the same pinned rev as the transitive dependency. (Previously: v0.2.10 was pinned; it introduced ChangeGate + register_gated for per-projection change-gating. Crates.io publish was skipped because workspace path deps lacked versions, so git-rev pin was the delivery mechanism.)

<!-- citations: [^c43d5-4] [^c1691-59] [^c1691-75] [^c1691-160] [^c1691-209] [^c1691-288] -->
## Upgrade History

The upgrade from v0.2.1 to v0.2.2 was a pure pin bump with no code changes needed, because the C-ABI breaking rename of timeline_insert_events → timeline_insert_event_batch did not affect the podcast app. <!-- [^c43d5-5] -->


All 7 open GitHub issues were addressed and merged via PRs #342, #344, #345, #348, #346, #347, #350. <!-- [^c33b9-4] -->
## Stale PRs

PR #245 (FeedbackStore WebSocket migration) is superseded by #248 and should be closed. <!-- [^c43d5-6] -->

## Remote Package References

The ios-shake-feedback dependency is referenced as a remote package from https://github.com/pablof7z/ios-shake-feedback with the requirement .upToNextMajor(from: "1.0.0") so the project can be built on other machines without a local sibling checkout. <!-- [^52b66-1] -->

## NMP Extension Migration

An issue should be filed in the ios-shake-feedback repo (not the podcast-player repo) to track converting ios-shake-feedback into an NMP extension/plug-in that any NMP app can integrate by adding the package and calling a single register entry point. <!-- [^52b66-2] -->

Issue #270 (filed in the podcast-player repo) for migrating ShakeFeedbackCore into NMP was merged and closed. PR #269 (updating Project.swift to use the remote ios-shake-feedback reference) was merged into main. <!-- [^52b66-3] -->

## Audit Follow-Up Backlog

Four NMP audit follow-up backlog entries (batchSize pacing, connectedAt stamp sites, unconditional rev bump, pop-then-if-let staging gap) are tracked under the P2 'Cross-Cutting Technical Debt' section in docs/BACKLOG.md via PR #370. The BACKLOG has no P3 section, so P2/P3-grade items are placed under P2. <!-- [^38f81-7] -->


Relay-config persistence via the C-ABI path is already done (commit 0dcf9680), loading from .nmp-relay-config.json at register and saving on edit; the BACKLOG entry and the register.rs comment block at lines 233–262 are stale. <!-- [^c1691-182] -->
## Template Defaults Cleanup

Docs must stop using old bundle/template names where the fix is straightforward. README.md must be rewritten to replace stale template placeholders and fix the Xcode 15.0+ requirement that conflicts with the iOS 26 deployment target. Template defaults (organizationName 'Your Company', AppTemplate in archive/secrets scripts) must be renamed or made explicit via environment variables. Docs now use the current Podcastr bundle IDs and app group instead of old template placeholders.

<!-- citations: [^rollo-57] [^rollo-134] -->

## NMP Version Upgrades

NMP migration doctrines (e.g., 'Rust owns business logic') apply only to NMP migration work, not to every current Swift feature fix unless explicitly part of the migration. <!-- [^rollo-190] -->

## Storage Backend

The store is JSON-backed rather than the plan's `sled`. <!-- [^rollo-227] -->

## Feature Parity Plan

`docs/plan/nmp-feature-parity.md` must be updated to add per-feature status as `done / partial / scaffold / wrong / blocked`. <!-- [^rollo-228] -->
