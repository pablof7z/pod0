# Agent Skills System — Built-in Podcast Generation Skill

## Context

Today the agent always sees every podcast tool — including `generate_tts_episode` and `configure_agent_voice` — and the system prompt always carries a paragraph teaching it how to compose TTS episodes. That makes the prompt heavier than it needs to be on most turns (the user rarely asks for a generated episode) and there's no mechanism to surface deeper, focused instructions only when relevant.

We want a generic **skill** mechanism: the agent has a meta-tool, `use_skill(skill_id)`, that opts the current conversation into a named skill. Activating a skill (a) returns a focused instruction manual as the tool result and (b) unlocks a small set of skill-specific tools for the rest of the conversation. Skills are listed by name + one-liner in the system prompt so the agent knows what's available without burning context on full manuals.

The first concrete skill is **`podcast_generation`** — it bundles the existing TTS authoring tools, adds a new `list_available_voices` tool, and teaches the agent how to structure turns so chapters auto-generate correctly.

## High-level shape

- **Always-on**:
  - `use_skill` tool (meta-tool to opt into a skill)
  - System prompt lists available skills (id + one-line description)
- **`podcast_generation` skill** (opt-in via `use_skill`):
  - Instructions manual (turn structure, chapter mapping, voice selection, multi-speaker dialogue, emotion cues, snippet handling)
  - Tools: `generate_tts_episode`, `configure_agent_voice`, `list_available_voices` (new)
- Skill enabled-state lives on the session and persists in `ChatConversation`.
- Tool schema sent to the LLM each turn = base + (skill schemas for each enabled skill).
- Gating is enforced in exactly one place — the dispatcher — via `AgentSkillRegistry.owningSkillID(forTool:)`. The schema-per-turn is what *keeps the LLM from calling* a gated tool; the dispatcher check is the defensive belt-and-suspenders.

## Routing model

Important constraint surfaced during review: `AgentTools.dispatch` (`AgentTools.swift:65-83`) uses `PodcastNames.all.contains(name)` as the routing predicate for sending a tool name to `dispatchPodcast`. We must keep `generate_tts_episode` and `configure_agent_voice` inside `PodcastNames.all` — otherwise even the happy-path skill-enabled call returns "Unknown tool". Only the **schema** entries move out of `AgentToolSchema+Podcast.swift` into the skill definition; the name constants, `PodcastNames.all` membership, and `dispatchPodcast` switch cases stay put.

## New files

(All under `App/Sources/**` or `AppTests/Sources/**` — Tuist's `Project.swift` globs both directories with `**`, so no project-file edit is needed.)

### 1. `App/Sources/Agent/Skills/AgentSkill.swift`
Defines the value type for a skill:
```swift
struct AgentSkill: Sendable {
    let id: String                          // e.g. "podcast_generation"
    let displayName: String                 // human-readable
    let summary: String                     // one-line, used in system prompt catalog
    let manual: String                      // full instructions returned by use_skill
    let toolNames: [String]                 // tool name constants this skill exposes
    let schema: @MainActor () -> [[String: Any]]  // tool schemas for the LLM
}
```
Plus `AgentSkillID` namespace with string constants (`podcastGeneration = "podcast_generation"`).

### 2. `App/Sources/Agent/Skills/AgentSkillRegistry.swift`
```swift
enum AgentSkillRegistry {
    @MainActor static var all: [AgentSkill] { [PodcastGenerationSkill.skill] }
    static func skill(id: String) -> AgentSkill? { ... }
    @MainActor static func schemas(for enabledIDs: Set<String>) -> [[String: Any]]
    static func toolNames(for enabledIDs: Set<String>) -> Set<String>
    static func owningSkillID(forTool name: String) -> String?
    static var allToolNames: Set<String> { ... }    // every skill-gated tool name
}
```

### 3. `App/Sources/Agent/Skills/PodcastGenerationSkill.swift`
Defines the `podcast_generation` skill instance. Holds:
- `summary` — "Create custom audio podcast episodes with TTS + audio snippets; auto-generates chapters and exposes the user's ElevenLabs voice library."
- `manual` — multi-section markdown covering:
  - **When to use** (TLDRs, summaries, mock interviews, compilation episodes)
  - **Turn structure** — `speech` vs `snippet`; ordering matters; min 1 turn
  - **Chapter generation** — consecutive `speech` turns collapse into one chapter (title = first ~60 chars of combined text); each `snippet` turn becomes its own chapter using the source episode's artwork; all chapters are flagged `isAIGenerated`; chapter timestamps come from concatenated turn durations
  - **Voice selection** — call `list_available_voices` first; pick by name/gender/accent; multi-speaker dialogue alternates `voice_id` per turn; `configure_agent_voice` sets the default for omitted `voice_id`s
  - **Emotion cues** — ElevenLabs supports `[cheerfully]`, `[excitedly]`, `[laughs]`, etc. in `text`
  - **Snippets** — resolve `episode_id`, `start_seconds`, `end_seconds` via `query_transcripts` or chapter lists before calling
  - **Quality gate** — always call `upgrade_thinking` before drafting a complex multi-turn script
- `toolNames` — `[generateTTSEpisode, configureAgentVoice, listAvailableVoices]`
- `schema()` — returns the three JSON schemas: the two moved from `AgentToolSchema+Podcast.swift` (lines 345–395) plus a new `list_available_voices` schema (one optional `query` arg).

### 4. `App/Sources/Agent/AgentTools+Voices.swift`
Implements `listAvailableVoicesTool(args:)`:
- Reads the API key via `ElevenLabsCredentialStore.fetchAPIKey()` (already used elsewhere in the codebase).
- Calls `ElevenLabsVoicesService().fetchVoices(apiKey:)` (`App/Sources/Features/Settings/AI/ElevenLabsVoicesService.swift:66`).
- Optional `query` filter applied client-side against `voice.searchText`.
- Returns up to 30 voices as JSON: `voice_id`, `name`, `category`, `gender`, `accent`, `description`, `preview_url`.
- Wires into `dispatchPodcast`'s switch (new case `PodcastNames.listAvailableVoices`).

### 5. `AppTests/Sources/AgentSkillsTests.swift`
Unit coverage (see "Verification" for the assertions list).

## Modified files

### 6. `App/Sources/Agent/AgentTools.swift`
Consolidated changes in one file:
- Add `Names.useSkill = "use_skill"`.
- Add `PodcastNames.listAvailableVoices = "list_available_voices"` and include in `.all`.
- Change `dispatch(...)` to accept a new `enabledSkills: Set<String> = []` parameter; forward into `dispatchPodcast`.
- `use_skill` is NOT routed through `dispatch` — it's intercepted in the turn loop (precedent: `upgrade_thinking` at `AgentTools.Names.upgradeThinking` comment).

### 7. `App/Sources/Agent/AgentToolSchema.swift`
- Append a `use_skill` tool entry to `schema`.
- Description: "Opt this conversation into a skill listed under '## Skills' in the system prompt. The tool result returns the skill's manual and unlocks its tools for the rest of the conversation. Idempotent — re-calling is harmless."
- One arg: `skill_id: string` (required).

### 8. `App/Sources/Agent/AgentToolSchema+Podcast.swift`
- Remove the `generate_tts_episode` schema entry (lines 345–381) and the `configure_agent_voice` schema entry (lines 382–395).
- They move to `PodcastGenerationSkill.swift`'s `schema()` closure.
- **Do not** remove their names from `PodcastNames.all` (see "Routing model" above).

### 9. `App/Sources/Agent/AgentTools+Podcast.swift`
- Add `case PodcastNames.listAvailableVoices: return await listAvailableVoicesTool(args: args)` in `dispatchPodcast`.
- Add `enabledSkills: Set<String> = []` parameter to both `dispatchPodcast` overloads; thread through.
- Before the existing switch, defensive gate: if `AgentSkillRegistry.owningSkillID(forTool: name)` is non-nil and that ID isn't in `enabledSkills`, return `toolError("Tool '<name>' requires the '<owningID>' skill — call use_skill first.")`. Single gating point per advisor feedback.
- Keep `generateTTSEpisode` and `configureAgentVoice` in `PodcastNames.all` and their dispatch cases unchanged.

### 10. `App/Sources/Agent/AgentPrompt.swift`
- Delete the existing "You can create custom audio episodes with `generate_tts_episode`..." paragraph (lines 34–41).
- Add a `## Skills` section after the opening paragraph listing each `AgentSkillRegistry.all` entry as `- <id> — <summary>`, followed by: "Call `use_skill(skill_id=…)` to load any of these — you'll get its full instructions back and unlock its tools."

### 11. `App/Sources/Features/Agent/AgentChatSession.swift`
- Add `var enabledSkills: Set<String> = []`.
- In the auto-resume branch (line 93–99), restore from `recent.enabledSkills`.

### 12. `App/Sources/Features/Agent/ChatConversation.swift`
- Add `var enabledSkills: Set<String>` (default `[]`).
- Update memberwise init to accept it with default `[]`.
- Codable: rely on auto-synthesis; since `Set<String>` is Codable and we keep adding via `decodeIfPresent` patterns elsewhere — quick check whether `ChatConversation` uses synthesized Codable or custom. If synthesized, the new property must be optional or have a `decodeIfPresent` wrapper via custom Codable, OR we add a custom decoder for backwards compat with old JSON. (Memory backed by `ChatHistoryStore` — see `App/Sources/Services/ChatHistoryStore.swift:115` for the existing legacy-fallback decoding pattern.)

### 13. `App/Sources/Features/Agent/AgentChatSession+Conversations.swift`
- Round-trip `enabledSkills` through `snapshotConversation()` and `loadConversation()` (mirrors existing `isUpgraded` handling at lines 19 and 54).
- Reset to `[]` on new-conversation paths (same place `isUpgraded = false` at line 83).

### 14. `App/Sources/Features/Agent/AgentChatSession+Turns.swift`
At line 156 the request is built. Change:
```swift
tools: AgentTools.schema + AgentTools.podcastSchema,
```
to:
```swift
tools: AgentTools.schema
     + AgentTools.podcastSchema
     + AgentSkillRegistry.schemas(for: enabledSkills),
```
At the tool-dispatch branch (line 235 where `upgrade_thinking` is intercepted), add an analogous interception for `use_skill`:
```swift
} else if toolCall.name == AgentTools.Names.useSkill {
    resultJSON = handleUseSkill(argsJSON: toolCall.arguments)
}
```
`handleUseSkill` parses `skill_id`, validates against the registry, inserts into `enabledSkills`, and returns `{ "success": true, "skill_id": ..., "manual": ..., "tools_unlocked": [...] }`. If unknown id, returns `toolError("Unknown skill: ...")`.

Pass `enabledSkills` into `AgentTools.dispatch(...)` so the defensive gate in `dispatchPodcast` has the context it needs.

### 15. `App/Sources/Agent/AgentRelayBridge.swift`
Mirror the same two changes (Nostr-headless reply path):
- Add `var enabledSkills: Set<String> = []` local to `reply(...)`.
- Same `+ AgentSkillRegistry.schemas(for: enabledSkills)` in the tools list (line 58).
- Same in-band interception for `use_skill` (line 93–98).

## Reusable code

These existing pieces are used unmodified:
- `App/Sources/Agent/AgentTTSComposer.swift` — composition / stitching / chapter building (the manual just describes its turn→chapter mapping).
- `App/Sources/Agent/AgentTools+TTS.swift` — `generateTTSEpisodeTool` and `configureAgentVoiceTool` handlers stay where they are; only the schema entries move.
- `App/Sources/Features/Settings/AI/ElevenLabsVoicesService.swift` — `ElevenLabsVoicesService.fetchVoices(apiKey:)` powers `list_available_voices`.
- `ElevenLabsCredentialStore` — provides the API key for the voices call.
- The `upgrade_thinking` interception pattern in `AgentChatSession+Turns.swift:235-241` is the exact template for `use_skill` interception.

## File-length budget

- New `PodcastGenerationSkill.swift` will carry the manual string; expect ~150–200 lines. Keep under 300 (soft limit per `AGENTS.md`).
- `AgentSkill.swift` + `AgentSkillRegistry.swift` are small (~60 lines each).
- `AgentToolSchema+Podcast.swift` shrinks by ~50 lines (two tool entries removed). Still well under 500.

## Verification

End-to-end checks before merge:

1. **Generate & build**: `tuist generate && tuist build` (per `README.md:210, 223`). Project sources auto-globbed via `App/Sources/**`, so the new `Skills/` subdirectory is picked up without project-file edits.

2. **Existing tests** (`AppTests/Sources/`):
   - `AgentToolsPodcastTests.swift:12` (`testPodcastSchemaListsEveryToolName`) currently asserts `Set(podcastSchema names) == Set(PodcastNames.all)`. After the move, this fails. **Update the assertion** to exclude the migrated tools (e.g. `expected.subtracting(AgentSkillRegistry.allToolNames)`), so the test reflects the new contract: `podcastSchema` covers `PodcastNames.all` minus skill-gated tools.
   - Other tests in `AgentToolsPodcastTests.swift`, `AgentToolsPodcastActionTests.swift`, `AgentToolsPodcastMocks.swift`, `PodcastSearchTests.swift` should pass unchanged.

3. **New tests** (`AppTests/Sources/AgentSkillsTests.swift`):
   - `AgentSkillRegistry.schemas(for: [])` is empty.
   - `AgentSkillRegistry.schemas(for: ["podcast_generation"])` returns three entries (generate_tts_episode, configure_agent_voice, list_available_voices).
   - `owningSkillID(forTool: "generate_tts_episode") == "podcast_generation"`.
   - `dispatchPodcast(name: "generate_tts_episode", args: [...], deps: mock, enabledSkills: [])` returns an error JSON containing "requires the 'podcast_generation' skill".
   - Same call with `enabledSkills: ["podcast_generation"]` reaches the existing handler (passes through to the dep mock).
   - Round-trip a `ChatConversation` with `enabledSkills = ["podcast_generation"]` through JSON encode/decode preserves the set.
   - Decode an old `ChatConversation` JSON snapshot (no `enabledSkills` field) → defaults to `[]`.

4. **Runtime smoke test** (simulator, ElevenLabs key configured):
   - Open the agent sheet, ask "what skills are available?" — should mention `podcast_generation` (from prompt's `## Skills` section).
   - Ask "what voices can I use?" — agent should call `use_skill(podcast_generation)`, then `list_available_voices`, then reply with voice options.
   - Ask "make me a 30-second TLDR with voice X" — should compose, call `generate_tts_episode`, episode appears in the "Agent Generated" subscription with auto-generated chapters.
   - Start a new conversation — `enabledSkills` resets to `[]`; the agent must opt-in again.
   - Force-quit the app mid-conversation, relaunch within 3h — auto-resumed conversation retains `enabledSkills`.

5. **Whats-new entry**: per `AGENTS.md` line 5, add a one-liner to `App/Resources/whats-new.json` for the commit shipping the change (e.g. "Agent now has opt-in skills, starting with podcast generation").

## What we are NOT doing

- Not adding a Settings UI for skills (out of scope — the agent enables skills itself in-band; a future skill toggle UI is a separate change).
- Not refactoring the existing 34 podcast tools into skills wholesale — only TTS/voice tools migrate now. Wiki, briefing, etc. stay always-on. Once the skill mechanism proves itself we can move more.
- No persisted "default enabled skills" — every new conversation starts with `enabledSkills = []` so the prompt stays lean by default.
