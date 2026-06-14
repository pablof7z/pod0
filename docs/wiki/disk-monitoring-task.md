---
title: Disk Monitoring Task
slug: disk-monitoring-task
topic: data-persistence
summary: The disk monitoring task triggers cleanup when free space drops below 15 GB
tags:
  - capture
volatility: warm
confidence: medium
created: 2026-06-13
updated: 2026-06-14
verified: 2026-06-13
compiled-from: conversation
sources:
  - session:16ac1219-405e-4d37-bcba-f2ad417a7e1e
  - session:c1691db0-d63e-4062-adad-1cfa0d679d09
---

# Disk Monitoring Task

## Disk Monitoring Task

The disk monitoring task triggers cleanup when free space drops below 15 GB. The task targets at least 80 GB of free space after cleanup. The task must not lose anything important during cleanup. Proactively clear /tmp scratch and main target/ directories when disk falls below 15 GB to prevent ENOSPC build failures. Build artifacts are located in Library, ~/src, and ~/Work. Android Gradle builds consume ~10 GB of disk due to cargo-ndk cross-compilation; disk must be proactively managed and implementors must cargo clean when space is low. (Previously: cleanup triggered when free space dropped below 5 GB.)

<!-- citations: [^16ac1-2] [^c1691-307] [^c1691-351] -->
