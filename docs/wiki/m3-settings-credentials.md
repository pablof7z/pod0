---
title: M3 Settings and Credentials Migration
slug: m3-settings-credentials
summary: "M3 migrates all settings and credentials into the Rust kernel: SettingsSnapshot expansion, iCloudSettingsSync deletion, and OpenRouter/Ollama to PcstIdentityCapability Keychain slots."
tags:
  - m3
  - settings
  - credentials
  - rust
  - migration
volatility: warm
confidence: medium
created: 2026-05-30
updated: 2026-06-02
verified: 2026-05-30
compiled-from: conversation
sources:
  - session:14943b9b-5bf3-4317-bc44-298a773bc75e
  - session:8bfa1b91-b40c-44b3-acb9-245b36f4c841
---

# M3 Settings and Credentials Migration

> M3 migrates all settings and credentials into the Rust kernel: SettingsSnapshot expansion, iCloudSettingsSync deletion, and OpenRouter/Ollama to PcstIdentityCapability Keychain slots.

## Overview

M3 moves all settings and credentials into the Rust kernel. The goal is to delete iCloudSettingsSync and migrate OpenRouter/Ollama credentials to the PcstIdentityCapability Keychain slots. The migration-v2.md done-when criteria: no @AppStorage domain settings, all settings projected with ops, iCloudSettingsSync deleted, OpenRouter/Ollama to Keychain. The Rust kernel's `podcast_keys` store persists to disk via write-through saves to `podcast-keys.json` in the app data directory — `save()` is called after every key mutation using an atomic tmp+rename strategy. `set_data_dir` must be called at startup to bind the store to the data directory and load any persisted keys.

<!-- citations: [^14943-88] [^14943-147] [^8bfa1-3] -->
## Current State (~40% done)

Prior to M3, settings are split across two systems. The old iCloudSettingsSync is still live at AppStateStore.swift:234,261,282,288 and AppStateStore+MutationBatch.swift:61, syncing settings via iCloud. The new iCloudSyncCapability only syncs 5 of ~55 settings fields. Settings.swift has ~55 fields; SettingsSnapshot (the Rust projection) has only 8. The updateSettings host-op dispatches only a few settings; most are Swift-only. Credentials: OpenRouterCredentialStore and OllamaCredentialStore still use Swift keychain wrappers rather than the PcstIdentityCapability Keychain slots. <!-- [^14943-89] -->

## Workstreams

M3 has two distinct but parallelizable workstreams: (1) SettingsSnapshot expansion — all ~55 settings fields are projected to the Rust kernel, with corresponding host-ops for the Swift UI to dispatch. iCloudSettingsSync is deleted, replaced by one-way iCloudSyncCapability reporting that covers all projected fields. (2) Credential migration — OpenRouterCredentialStore and OllamaCredentialStore are thin shims delegating to PcstIdentityCapability.direct, with public API unchanged. PcstIdentityCapability uses a nonisolated(unsafe) static let direct singleton for synchronous credential access from non-MainActor contexts, bypassing the started guard since Keychain operations are stateless. LegacyKeychainMigration uses a v2 sentinel (not v1) when adding new migration slots, because the v1 sentinel already fired for existing users; skip-if-present guards make the batch idempotent. The legacyOpenRouterAPIKey is removed from the Swift compat layer. (3) Nostr event signing — kind:0/1/9802 Nostr event signing is moved to the Rust kernel, wired through the user identity bridge added via podcast.identity ImportNsec.

<!-- citations: [^14943-90] [^14943-110] [^14943-148] -->
