---
title: Nostr Relay Config Persistence
slug: nostr-relay-config-persistence
topic: nostr-protocol
summary: Relay config persistence via the C-ABI path is already implemented (commit 0dcf9680)
tags:
  - capture
volatility: warm
confidence: medium
created: 2026-06-13
updated: 2026-06-13
verified: 2026-06-13
compiled-from: conversation
sources:
  - session:c1691db0-d63e-4062-adad-1cfa0d679d09
---

# Nostr Relay Config Persistence

## Relay Config Persistence (C-ABI Path)

Relay config persistence via the C-ABI path is already implemented (commit 0dcf9680). The load logic resides in `ffi/data_dir.rs` and the save logic in `host_op_handler/settings_actions.rs` → `ffi/relay_persist.rs`. The BACKLOG entry and the stale comment block in `register.rs` should be updated to reflect this completed work. <!-- [^c1691-334] -->
