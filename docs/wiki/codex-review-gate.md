---
title: Codex Review Gate
slug: codex-review-gate
summary: An Opus agent review is the required approval gate before shipping; it must return a clean verdict with no P1 findings.
tags:
  - codex
  - review
  - gate
  - quality
  - approval
volatility: cold
confidence: medium
created: 2026-05-29
updated: 2026-06-02
verified: 2026-05-29
compiled-from: conversation
sources:
  - session:14943b9b-5bf3-4317-bc44-298a773bc75e
  - session:8bfa1b91-b40c-44b3-acb9-245b36f4c841
---

# Codex Review Gate

> An Opus agent review is the required approval gate before shipping; it must return a clean verdict with no P1 findings.

## Gate Requirement

Opus agents are used for code reviews. Before shipping any migration or significant change, a code review must be run and must return a clean approval. This is an Opus agent review, not a CLI command. It audits the diff against `main` for correctness, security, maintainability, technical debt, gimmicks, shortcuts, and alignment with NMP.

<!-- citations: [^14943-38] [^14943-105] [^8bfa1-1] -->
## Approval Criteria

The review returns findings at P1 (blocking) and P2 (non-blocking) severity. For the gate to be satisfied, the review must return with no P1 findings. P2 findings may be addressed or rebutted with evidence, but the review must finish with no actionable correctness issues. A clean verdict reads: "No introduced correctness, security, or maintainability issues were found in the reviewed diff." <!-- [^14943-39] -->

## What the Review Checks

Review checks include:
- D5 wire-contract byte-identity (Rust tests: `cargo test -p nmp-app-podcast --lib` must pass all tests)
- FFI boundary correctness (symbol types, memory ownership, ABI compatibility)
- Report channel completeness (all report types wired consistently)
- Decode strictness (default-tolerant but not silently swallowing errors)
- Dead code and gimmicks (`if false` blocks, `while false` loops, unused poll loops)
- Contract drift (header/implementation mismatch)
- Disk/resource awareness (flags near-full volumes) <!-- [^14943-40] -->

## Review Lifecycle

The review runs as a background agent process (~7 minutes). It may be re-run multiple times as issues are addressed. Each re-run should be on the current branch state with all prior findings addressed. The review must be allowed to complete — do not claim victory until the verdict is returned. If the review fails with an error (e.g., exit 101 from `ENOSPC`), the environment must be fixed (e.g., free disk space) and the review re-run. <!-- [^14943-41] -->

A convergence rule governs multiple rounds of review. Round 1 surfaces architectural-class findings (P1); round 2 surfaces deeper issues; round 3 typically finds localized P2 logic nits in the round-2 fixes themselves; round 4 is the signal to stop iterating. After round 4, only localized, fixable issues should remain — architectural-class findings would be a sign to checkpoint rather than continue. The M2 feature went through 5 review passes (initial + 3 rounds of findings + 2 cleanup rounds), with all findings being real but increasingly localized edge cases in the same feature. The final P2 (cold-launch isOnWifi initialization) was a single-line fix — evidence of convergence, not a design flaw. <!-- [^14943-83] -->

<!-- citations: [^14943-41] [^14943-106] -->
## Rebutting Findings

Findings can be rebutted with evidence. For example, when codex flagged `ChaptersClientDecodeTests.swift` as missing, the rebuttal showed that `ChaptersClient` was deleted in commit `fa522f87` (chapter decoding moved to Rust) and coverage lives in `apps/podcast-feeds/src/podcasting2/chapters_tests.rs`. Restoring the test would break the build. A documented, evidence-backed rebuttal is acceptable. <!-- [^14943-42] -->


## Merge Procedure

When merging an approved PR, use `gh pr merge --merge` (not `--squash` or `--rebase`). Enable auto-merge rather than admin-bypassing a pending CI check — the merge lands the moment the build goes green. After merge, delete the remote branch, fast-forward local main, remove the worktree, and update WIP.md (move the entry from Active to Recent History). <!-- [^14943-84] -->

## See Also
- [[d5-wire-contract|D5 Wire Contract and Swift Decode Resilience]] — related guide
- [[nmp-v0-1-0-adoption|NMP v0.1.0 Adoption]] — related guide
- [[live-testing-methodology|Live Testing Methodology]] — related guide
- [[agent-and-social-protocols|Agent-to-Agent and Social Protocols]] — related guide

