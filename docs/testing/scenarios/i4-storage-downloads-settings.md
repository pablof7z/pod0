# Scenario I4: Storage, downloads, and data settings

## Goal
Validate the Data & Storage settings: downloads manager, storage summary, data
export, and clear-all-data.

## Prerequisites
- App past onboarding with ≥1 downloaded episode (C3) for a meaningful storage
  summary.

## Steps
1. Settings → Listening → **Downloads** (DownloadsManagerView, `downloads-list`).
   **Expected:** A list of downloads with state/progress; manage/remove affordances.
   *Screenshot.*
2. Settings → System → **Data & Storage**. **Expected:** "Export Data" (shows
   "N records"), "Downloads & Disk" (shows a storage summary like "2.4 GB"), and a
   "Clear All Data" destructive button. *Screenshot.*
3. Tap **Downloads & Disk** (StorageSettingsView). **Expected:** Per-storage detail.
   *Screenshot.*
4. Tap **Export Data** (DataExportView). **Expected:** An export/share path with the
   record count. *Screenshot.*
5. (Teardown only) Tap **Clear All Data** → confirm the alert. **Expected:** Wipes
   subscriptions/episodes/notes/friends/memory but PRESERVES API credentials and the
   Nostr identity. *Screenshot.* (Skip unless you intend to reset.)

## Acceptance Criteria
- Downloads manager lists downloads and allows removal.
- Storage summary reflects actual downloaded bytes.
- Data export offers the data with an accurate record count.
- Clear All Data wipes content but preserves credentials + identity (per the footer).

## Known Issues / Watch Points
- "Clear All Data" is destructive — only run as deliberate teardown. It preserves
  identity/keys by design; verify the identity survives if you do run it.
- Storage figures are kernel-reported; a wildly wrong number is worth noting.

## Notes
