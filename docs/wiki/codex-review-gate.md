---
title: Codex Review Gate
slug: codex-review-gate
topic: project-setup
summary: All references to the deprecated 'codex exec review --base main' CLI must be replaced with Opus agent terminology across codex-review-gate.md, d5-wire-contract.
tags:
  - capture
volatility: warm
confidence: medium
created: 2026-06-02
updated: 2026-06-14
verified: 2026-06-02
compiled-from: conversation
sources:
  - session:8bfa1b91-b40c-44b3-acb9-245b36f4c841
  - session:c1691db0-d63e-4062-adad-1cfa0d679d09
---

# Codex Review Gate

## Terminology: Opus Agent vs. Deprecated CLI

All references to the deprecated 'codex exec review --base main' CLI must be replaced with Opus agent terminology across codex-review-gate.md, d5-wire-contract.md, m1-stack-integration.md, and disk-full-recovery.md.

Reviewers must never perform working-tree git operations (checkout, restore, stash, add) in the shared root; they use read-only `git diff`/`git show` from the object DB to avoid clobbering uncommitted work.

Opus reviews should git grep the entire workspace including podcast-tui when checking for orphan removals, because CI's -p nmp-app-podcast-scoped lint cannot catch breaks in other workspace members.

Opus review caught approximately 6 real defects that CI structurally could not, including a dead-on-arrival domain-rev bump, queue-row byte divergence, an untested action→re-emit seam, a workspace-TUI break from a narrow orphan grep, and false-confidence test assertions.

<!-- citations: [^c1691-337] [^8bfa1-1] [^c1691-17] [^c1691-36] [^c1691-132] [^c1691-203] [^c1691-230] [^c1691-251] [^c1691-401] -->
