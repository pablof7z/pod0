# Agent Guidelines

## Whats-new changelog

Every commit that ships a user-facing change to the iPhone MUST add an entry to `App/Resources/whats-new.json` with a one-liner the user will read. An entry needs only `shipped_at` (current UTC, ISO-8601) and `lines`. The app surfaces entries whose `shipped_at` is newer than the user's last-seen marker — no commit SHA needed. Timestamps must be unique across entries; if two land in the same minute, bump one by a minute. Skip entries for purely-internal commits (encoder caches, log line tweaks, formatting). When in doubt: would the user notice? If yes, add a line.

## Typography

**No serif fonts, ever.** Do not use `.serif` font design, `NewYork`, `NewYork-SemiboldItalic`, or any other serif typeface anywhere in the app. All text must use SF (system font). For italic style, use `UIFont.italicSystemFont` or `.italic()` modifier — never a serif variant.

## File Length Limits

- **Soft limit: 300 lines** — prefer splitting into smaller files when approaching this threshold.
- **Hard limit: 500 lines** — files must not exceed 500 lines. Refactor before adding more code.
