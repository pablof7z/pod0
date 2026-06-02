---
title: Disk Full Recovery (ENOSPC)
slug: disk-full-recovery
summary: "Procedure for recovering from a full macOS data volume (ENOSPC) that blocks builds and codex reviews. Primary space hogs: ~/.cargo/target-shared (83 GB) and stale DerivedData."
tags:
  - infrastructure
  - build
  - troubleshooting
volatility: cold
confidence: medium
created: 2026-05-30
updated: 2026-06-02
verified: 2026-05-30
compiled-from: conversation
sources:
  - session:14943b9b-5bf3-4317-bc44-298a773bc75e
  - session:8bfa1b91-b40c-44b3-acb9-245b36f4c841
---

# Disk Full Recovery (ENOSPC)

> Procedure for recovering from a full macOS data volume (ENOSPC) that blocks builds and codex reviews. Primary space hogs: ~/.cargo/target-shared (83 GB) and stale DerivedData.

## Overview

The macOS data volume hitting 100% full (0 bytes free) blocks all operations: builds, codex reviews, and log writes fail with ENOSPC (exit 101). This occurred during the M1 codex review cycle. The primary space hogs are the iOS build cache (83 GB in ~/.cargo/target-shared) and stale Xcode DerivedData directories (~9 Podcastr-* dirs at ~7 GB total). <!-- [^14943-62] -->

## Recovery Steps

The recovery procedure: (1) Remove the iOS build cache: rm -rf ~/.cargo/target-shared. This reclaimed 83 GB. (2) Remove stale DerivedData dirs: rm -rf ~/Library/Developer/Xcode/DerivedData/Podcastr-* (keeping the active worktree's hash). (3) After clearing, re-verify with df -h. Total reclaimed: ~90 GB. (4) Side effect: the iOS simulator static library must be rebuilt from scratch after clearing the cache, as it was the canonical source. This takes several minutes for the full NMP dependency graph. <!-- [^14943-63] -->

## Detection

ENOSPC is detected when Rust or Xcode builds fail with disk-full errors (I/O error or ENOSPC in cargo output), or when an Opus agent review cycle fails due to the volume being full. The `df -h` command confirms the volume at 100%.

<!-- citations: [^14943-64] [^8bfa1-2] -->
