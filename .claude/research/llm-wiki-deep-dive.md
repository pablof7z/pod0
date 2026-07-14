# Deep Dive: nvk/llm-wiki — and what to adapt for an iOS Podcast Player

> Research notes for the podcast-player wiki layer. Focused on what we should keep, what we should rethink, and what's risky.
> Sources fetched: see end of document.

---

## 1. What llm-wiki is, in plain words

[nvk/llm-wiki](https://github.com/nvk/llm-wiki) is a **plugin/skill that turns any agentic LLM (Claude Code, Codex, OpenCode, Pi) into a knowledge-base maintainer**. Not a runtime, not a server — a *protocol* (SKILL.md + ~19 slash commands + reference docs) telling the host LLM how to ingest, compile, query, and audit a directory of Markdown files on the user's disk.

Ethos, lifted from Karpathy's framing: **the LLM is the programmer, the wiki is the codebase, Obsidian is the IDE**. Where RAG re-derives an answer from raw chunks every query, an llm-wiki *compiles* synthesis once at ingest time into permanent, human-readable Markdown pages, then queries the compiled wiki. Knowledge compounds; humans curate sources and ask questions, the LLM does the maintenance bookkeeping no human will actually do. Target user: power-users of Claude Code / Codex who want a long-term research vault — Obsidian for people who'd rather have an LLM write the pages.

## 2. Architecture

**Tech stack:** ~94% shell, ~6% JS, **zero runtime dependencies** beyond the host agent's built-in tools (file read/write, web fetch, web search). All intelligence delegated to the host LLM via SKILL.md. The repo is essentially a giant carefully-versioned prompt.

**Data model — plain Markdown on disk.** The hub at `~/wiki/` (or a configured iCloud path) is a thin registry. Each topic is a self-contained sub-wiki:

```
~/wiki/topics/<topic>/
  raw/         immutable ingested sources, by type (articles/, papers/, repos/, notes/, data/)
  wiki/        synthesized articles → concepts/, topics/, references/, thesis/
  inventory/   durable tracking records (items, ingest-candidates, entities, corpora, questions, tasks)
  datasets/    manifests pointing at large external data (no copy)
  output/      generated artifacts (reports, slides, study guides)
  inbox/       drop zone
  .obsidian/   per-topic vault config
  _index.md, config.md, log.md
```

**Frontmatter schemas:**
- Raw sources: `title, source, type, ingested, tags, summary`
- Wiki articles: `title, category, sources, created, updated, tags, aliases, confidence (high|medium|low), summary` — and during compile, `volatility` and `verified` are required for the validation gate.
- Inventory: `title, kind, status, priority, created, updated, tags, summary, sources`.

**LLMs involved:** Whatever the host runtime provides. Claude Sonnet/Opus via Claude Code (~22K-token system prompt budget for the skill), GPT-class via Codex (~3K), local LLaMA via Pi (~1K). The plugin is intentionally model-agnostic.

## 3. Generation pipeline

No daemon — generation is **user-invoked via slash commands** that kick off agentic multi-step LLM workflows. Four triggers:

1. `/wiki:ingest <source>` — single URL/file/PDF/text → `raw/{type}/` → compile.
2. `/wiki:ingest --inbox` — batch on a drop folder.
3. `/wiki:ingest-collection` — bulk (Git repos, MediaWiki dumps, Wayback CDX, message archives).
4. `/wiki:research <topic|question>` — fans out **5 / 8 / 10 parallel research agents** (Standard / Deep / Retardmax) with distinct angles: academic, technical, applied, news, contrarian; +historical, adjacent, data; +rabbit-hole, first-link.

**Prompt template per agent:** Objective (angle + topic) · Focus & Strategy · Constraints (search volume, quality thresholds, skip rules) · Return Format (title, URL, quality score, key findings, ingestion rationale) · Quality Scoring Guide (5 peer-reviewed → 1 spam).

After fan-out: credibility review (peer-review/recency/author/bias/corroboration), URL+content dedup (>80% overlap), credibility-x-agent-quality ranking, top-N ingestion to `raw/`, then **compile**. Compile reads uncompiled sources, writes synthesized articles (abstract first, then body), bidirectionally cross-links, validates frontmatter, updates indexes, appends to `log.md`. Core line: *"Articles are synthesized, not copied — explain, contextualize, cross-reference."*

## 4. Cross-linking model

**Dual-link format on every cross-reference**, on a single line: `[[gut-brain-axis|Gut-Brain Axis]] ([Gut-Brain Axis](../concepts/gut-brain-axis.md))`. The cleverest mechanical decision in the project: `[[wikilink]]` powers Obsidian graph + backlinks; `(relative/path.md)` works in Claude Code, GitHub, plain viewers. **No tool lock-in.** Citations are inline links into `raw/{type}/...` files (source URL in frontmatter). Article confidence propagates from source credibility scores.

## 5. Update / freshness

Four mechanisms, all user-invoked (no daemon):

- `/wiki:refresh` — three-tier: re-fetch listed sources, extract 5–10 key facts, classify as unchanged/updated/contradicted/unreachable; assess impact (cosmetic/additive/contradictory); offer per-source actions **skip** (bump `verified` date) / **update** (replace raw + recompile) / **flag** (downgrade confidence) / **retract**.
- `--due` mode lists articles past a freshness threshold (default 70 days).
- `/wiki:librarian` — focused two-tier staleness+quality scan (fast metadata then deep on flagged).
- `/wiki:audit` — heaviest, "truth-seeking umbrella": traces outputs through wiki state to raw sources, may launch fresh research.

Re-generation is **incremental** by default; `/wiki:compile --full` recompiles everything. Cron-like refresh via `/loop 1d /wiki:refresh --due` while a session is active.

## 6. Editing model

Fully human-editable: plain Markdown in a directory. Versioning = whatever the user puts the dir under (iCloud, git). No lock or branch model. `/wiki:lint --fix` auto-repairs broken links, orphans, missing indexes. `raw/` is conceptually immutable — synthesized articles can be edited; raw sources should only be replaced via `/wiki:refresh update`. No edit-conflict model — single-user assumption is baked in.

---

## 7. What we should adapt for the podcast player wiki

**Data model — keep the three-layer separation, port to SQLite + Markdown export.**

| llm-wiki | Podcast player |
|---|---|
| `raw/` immutable sources | `transcripts/` immutable JSONs (text + word timings + diarization) |
| `wiki/` synthesized articles | concept / episode / show / person / cross-show pages |
| `inventory/` | watch-later, follow-this-thread, "track this guest" items |
| `datasets/` manifests | RSS feed manifests, OPML imports |
| `output/` | audio briefings, shareable clips, episode summaries |

Store articles as Markdown blobs in SQLite (FTS5 + separate vector index), **and** persist them as files in an iCloud-Drive folder the user can open in Obsidian on Mac. That single decision is what makes this feel like a personal knowledge base rather than a black-box "AI summary."

**Generation triggers — automatic, not user-invoked.** Unlike llm-wiki where the user types `/wiki:ingest`, ours fire silently:
1. **New episode published** → enqueue transcript fetch (publisher-provided if available, ElevenLabs Scribe otherwise) → enqueue compile.
2. **Transcript ready** → diarize → chunk → embed → page-update pass (not full recompile) on affected entity pages: show page, each speaker's person page, any concept page whose embedding centroid the new chunks fall near.
3. **User listens ≥X%** → optional "favorite-quote extraction."
4. **Agent tools**: `summarize_episode`, `query_wiki` (read-side, but writes interesting Q→A pairs back as new pages — Karpathy's "file valuable explorations back").

**Citation model — every claim points to `(episode_id, start_ms, end_ms)`.** Beyond llm-wiki, which only cites a URL. Each rendered sentence in the iOS UI has a tappable timestamp chip that calls `play_episode_at(episode_id, start_ms)`. Frontmatter:

```yaml
sources:
  - episode_id: ep_abc123
    show_id: show_tim_ferriss
    spans: [[1242000, 1268500], [3411000, 3433200]]
    speaker: tim_ferriss
    confidence: high
```

**Page taxonomy — per-podcast wikis + global cross-show layer.** llm-wiki's "one topic, one wiki" is right for a single show, but our killer feature is cross-show synthesis ("what does the podcast world say about Ozempic?"), so we add a **library-wide hub** with concept pages aggregated across shows, keyed by canonical entity (person, drug, book, place).

**On-device vs server.** On-device: query, RAG retrieval, rendering, agent loop, light summarization (Apple Foundation Models). Server (OpenRouter): compile, refresh, cross-show synthesis, embeddings. Default server, per-show "private" flag forces on-device. Worker: a `WikiCompiler` BGProcessingTask that wakes on charge+wifi, drains a `(episode_id, op)` queue. Mirror `log.md` as a `compile_events` table.

## 8. What we should rethink / redesign

- **Drop the slash-command UX.** Wiki pages are a tab on each show, a sheet on each episode, inline citations in agent answers. Same primitives (ingest, compile, query, refresh, audit) as agent tools and background jobs, never typed commands.
- **Replace the 5/8/10 web-research-agent pattern.** That's a Claude Code power-user feature. Our fan-out is per-episode: `extract_topics`, `extract_entities`, `extract_quotes`, `extract_action_items`, `link_to_existing_pages` — five parallel passes per new episode.
- **Confidence semantics shift.** llm-wiki's confidence is "source quality." Ours is "extraction confidence" — transcript right, diarization right, faithful synthesis. Cite the span; let the user tap-to-verify.
- **`thesis` mode unexpectedly relevant.** "Is intermittent fasting backed across all my podcasts?" maps cleanly. Keep it.
- **`refresh` model changes.** Transcripts don't change. What does: better transcription, better diarization, a guest later contradicting themselves. Refresh = re-embed with newer model, re-link entities, mark cross-episode contradictions.
- **Edit model.** Single user, multiple devices. iCloud last-write-wins + monotonic `compile_revision` per page. Not CRDT.
- **Editorial rendering.** Render via SwiftUI with editorial typography, tappable timestamp chips, parallax hero, Liquid Glass. The wiki is content; the app is the magazine layout over it.

## 9. Specific risks

- **Hallucination at synthesis.** Compile can drift from the raw span. Mitigation: every synthesized sentence carries a span pointer; a post-compile verification pass checks the cited span actually supports the claim (cheap classifier or LLM judge).
- **Attribution failure on diarization errors.** "Tim said X" when the guest said X is worse than no attribution. Mitigation: diarization-confidence threshold below which we attribute to "the show," not a named speaker.
- **Edit conflicts across devices.** iCloud + monotonic `compile_revision`. Don't pretend we solved CRDT.
- **Model cost.** A 90-min episode is ~12K tokens of transcript × multiple compile passes. Mitigations: cache, delta-compile, cheap models for extraction and premium for cross-show synthesis, per-tier limits.
- **Freshness lag.** Pure on-device means a fresh episode might wait days for a background task. Mitigation: server pre-computes transcript + embeddings + extraction on RSS hit; device pulls compiled deltas.
- **Copyright / fair use.** Full transcripts of paid shows are gray; synthesized pages with quotes <125 chars (llm-wiki's cap) are safer. Mirror the cap.
- **RAG bypass.** Karpathy's whole point is "compile, don't re-RAG." The agent will be tempted to query transcript chunks when the wiki lacks an answer. Decide explicitly: wiki-first, transcript-RAG fallback, agent cites which it used.

## 10. Open questions

- **Compile cost per show?** Need dollars/episode and seconds-of-compute for a Sonnet-tier model. llm-wiki is user-invoked and uncosted, so no signal there.
- **Page granularity.** Per-episode (always), per-show evergreen, per-person, per-concept — but where do "thread" pages live (the running argument across 8 episodes)? No llm-wiki analogue.
- **Vector index choice.** SQLite-vec? sqlite-vss? Apple NLEmbedding? llm-wiki sidesteps embeddings (full-text + agent reasoning) — suspicious at podcast scale.
- **Multi-user / family sharing.** llm-wiki is single-user. Do we want shared wikis via Nostr DMs? The template already has Nostr agent comms.
- **Briefing audio integration.** llm-wiki's `/wiki:output` is text. Our interruptible TTS briefing from a wiki traversal — agent tool, separate generator, or dedicated `BriefingComposer`?
- **How does llm-wiki actually score 80%-overlap dedup?** Shell + LLM judgment, not deterministic. Worth a closer read of `compile.md` before we copy.
- **`_index.md` everywhere** — does it scale to 200 shows × 50 episodes? In SQLite, treat indexes as views, materialize only on iCloud export.

---

## URLs fetched

- https://github.com/nvk/llm-wiki — project page (default branch `master`)
- https://api.github.com/repos/nvk/llm-wiki/contents/ — root file tree
- https://api.github.com/repos/nvk/llm-wiki/contents/claude-plugin/commands — 19 command files
- https://raw.githubusercontent.com/nvk/llm-wiki/master/README.md (~34KB) · `/CLAUDE.md` · `/AGENTS.md` (~36KB)
- `claude-plugin/commands/{ingest,compile,query,refresh,research}.md`
- https://llm-wiki.net/ — project site
- https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f — Karpathy's original LLM-wiki concept
- https://denser.ai/blog/llm-wiki-karpathy-knowledge-base/ — third-party explainer
- WebSearch: "nvk llm-wiki github project"

---

**File path:** `/Users/pablofernandez/Work/podcast-player/.claude/research/llm-wiki-deep-dive.md`
