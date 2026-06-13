---
title: Auto Skip Ads
slug: auto-skip-ads
topic: playback
summary: "auto_skip_ads defaults to true (Previously: false.) When enabled, ads that are properly labeled in the chapter list are automatically skipped during playback"
tags:
  - capture
volatility: warm
confidence: medium
created: 2026-05-13
updated: 2026-06-13
verified: 2026-05-13
compiled-from: conversation
sources:
  - session:0f3f24f7-54de-49f8-b160-a92f735f6a00
  - session:dced2b33-dfba-41f2-b631-a0dffd418d59
  - session:c1691db0-d63e-4062-adad-1cfa0d679d09
---

# Auto Skip Ads

## Settings

auto_skip_ads defaults to true (Previously: false.) When enabled, ads that are properly labeled in the chapter list are automatically skipped during playback. PersistedSettings uses #[serde(default = "default_true")] for auto_skip_ads_enabled so JSON files written before the field existed hydrate as true; users who explicitly set false are unaffected since serde only invokes the default when the key is absent.

<!-- citations: [^0f3f2-16] [^dced2-1] [^c1691-149] -->

## Shipped Rails

Android AI-chapters and auto-skip-ads rails are shipped: EpisodeDetailScreen dispatches podcast.chapters compile, SettingsScreen toggles auto_skip_ads_enabled via podcast.settings set_auto_skip_ads, and PlayerScreen renders amber ad-segment markers on the seek bar. Empty ad vectors are filtered on disk (`disk.rs:357` `.filter(|(_,v)| !v.is_empty())`), so the ad-detection-ran gate resets across process restart for no-ad episodes, causing one cheap re-run per cold start.

<!-- citations: [^c1691-150] [^c1691-278] -->
