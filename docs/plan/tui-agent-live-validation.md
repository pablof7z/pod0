# TUI Agent Live Validation

**Goal:** validate agentic podcast workflows by driving the real
`podcast-tui` inside tmux, with real kernel state and Ollama Cloud model
settings. No fake action dispatch, stubbed agent calls, or headless-only
substitutes count for this pass.

## Preconditions

- The TUI must run from a dedicated data directory so test state is isolated.
- The terminal process must either have `OLLAMA_API_KEY` set before launch or
  a locally authenticated Ollama daemon with `:cloud` models available.
- For local Ollama Cloud validation, `ollama ls` must show the selected
  `:cloud` model and the TUI Ollama chat URL must point at that daemon.
- The TUI Settings `providers` section should load provider keys from env when
  cloud providers need explicit keys; authenticated local Ollama does not need
  a raw key in the TUI settings projection.
- All LLM role rows should be set to Ollama Cloud-compatible model IDs:
  agent initial, agent thinking, memory compilation, wiki, categorization,
  chapter compilation, embeddings, and image generation where the kernel can
  honor the role.
- Test evidence should come from tmux pane captures and persisted app state,
  not from direct kernel test calls.

## Exhaustive Agentic Scenario Inventory

### Discovery And Library Work

- Find podcasts by topic, host, title, publisher, or guest.
- Subscribe to a discovered podcast.
- Explain why a selected show is relevant to the user's interests.
- Compare two or more search results and recommend one.
- Find recent unplayed episodes matching a theme.
- Find older saved episodes matching a theme.
- Identify episodes from a specific guest, creator, feed, or publication.
- Build a listening shortlist from the current library.
- Detect stale subscriptions that have not published recently.
- Suggest unsubscribing, archiving, or deprioritizing inactive feeds.

### Queue And Playback Work

- Add the best episode for a requested topic to the queue.
- Add multiple related episodes in a useful order.
- Play the selected episode.
- Pause, resume, seek, skip forward, or skip backward on request.
- Move from a general request to concrete queue actions.
- Clear the queue after confirmation.
- Remove a bad or already-heard item from the queue.
- Add an episode next instead of at the end.
- Explain what is currently playing.
- Recommend playback speed or skip behavior for a show.
- After a provider/model switch, ask the agent to queue a recommendation and
  verify the queue mutation still lands in the visible Queue tab.
- Queue, add-next, play, and remove an Agent Pick using only TUI actions.

### Inbox And Listening Triage

- Triage new inbox episodes by interest.
- Mark low-priority episodes played/listened.
- Keep high-priority episodes unread for later.
- Explain why an episode was kept or dismissed.
- Find inbox episodes related to active memory facts.
- Build a morning commute playlist from the inbox.
- Build a long-form weekend playlist from the inbox.
- Identify duplicate or near-duplicate news coverage.
- Surface episodes likely to include ads or filler.

### Memory And Preference Work

- Remember a durable user preference.
- Update an existing preference.
- Forget a stale preference.
- Use saved preferences to recommend episodes.
- Explain which memory facts influenced a recommendation.
- Store topic interests, avoided topics, favorite hosts, disliked shows,
  commute length, preferred episode length, and listening goals.
- Avoid over-writing unrelated memory facts.
- Clear all memory after explicit confirmation.
- Create a typed task that writes memory, run it, and verify the Memory section
  updates without needing a restart or manual refresh.
- Switch the active chat model after saving memory and verify memory context is
  still included in the next provider-backed response.

### Episode Understanding Work

- Summarize an episode.
- Answer questions about an episode transcript.
- Explain the key claims in an episode.
- Extract action items from an episode.
- Find a quote or segment by topic.
- Identify named people, companies, books, papers, or tools.
- Generate chapter titles from transcript content.
- Compare an episode to another episode or saved memory.
- Categorize an episode into user-facing topics.
- Explain whether an episode is worth finishing.

### Clips And Bookmarks Work

- Star/bookmark the current episode.
- Unstar a selected episode.
- Create a clip around the current playback position.
- Create a clip for a named topic if transcript/time data is available.
- Delete an unwanted clip.
- Explain the context of a saved clip.
- Build a list of notable clips from recent listening.
- Turn a clip into a social/share draft when identity support exists.
- Verify bookmarks survive tab switches and remain actionable through play,
  queue, add-next, and remove shortcuts.
- Verify clip creation reports a clear limitation when no episode is playing or
  when the current player position is unavailable.
- Verify a provider-backed agent turn can reference a bookmarked or clipped
  episode without hallucinating hidden transcript data.

### Downloads And Offline Work

- Download an episode for offline listening.
- Pause, resume, cancel, or delete a download.
- Queue downloads for travel.
- Identify episodes already available offline.
- Explain failed downloads and retry options.
- Keep downloads below a practical number for storage.

### Agent Chat Work

- Hold a conversational turn that references the current library.
- Ask follow-up questions when the request is ambiguous.
- Report inability clearly when a tool, key, identity, or transcript is absent.
- Avoid hallucinating actions that the kernel did not perform.
- Surface provider/model errors in the chat or status.
- Continue after a provider failure when possible.
- Preserve conversation context over several turns.
- Clear the chat when requested.

### Agent Picks Work

- Generate or render agent picks.
- Play a selected pick.
- Queue a selected pick.
- Add a selected pick next.
- Explain why each pick was chosen.
- Avoid picks that conflict with saved memory.
- Refresh picks after new subscriptions or new memory.

### Scheduled Agent Task Work

- Create a recurring task from natural language.
- Create one-shot tasks with a concrete schedule.
- Enable and disable an existing task.
- Run a task immediately.
- Delete a task.
- Explain what a task will do before saving.
- Reject malformed schedules or unsafe action JSON.
- Keep task list projection in sync after changes.
- Create each typed task intent exposed by the TUI after the typed-task branch
  lands, including memory, inbox/library triage, queue/playback, bookmarks,
  clips, notes/social, and wiki/summary intents where the backend supports
  them.
- Confirm the TUI task editor no longer requires raw namespace/body JSON after
  the typed-task branch lands, and that legacy raw input either disappears or
  fails with an explicit migration message.
- Confirm disabled typed tasks cannot run and do not mutate memory, queue,
  bookmarks, clips, or notes.
- Confirm run-now status moves through dispatched/running/completed or a clear
  error state and does not leave stale busy UI after provider failure.

### Agent Notes And Social Work

- Fetch inbound public agent notes.
- Publish an outbound note to a specified public key.
- Refuse note publishing when no identity is configured.
- Explain trust state for inbound notes.
- Avoid auto-responding to untrusted senders.
- Thread future conversation-style notes under a root when the kernel supports it.

### Wiki And Knowledge Work

- Search existing wiki articles.
- Generate a wiki article from a transcript or library topic when supported.
- Delete an obsolete wiki article.
- Explain which episodes support a wiki claim.
- Refresh a stale article when new evidence appears.
- Distinguish scaffolded wiki content from real RAG output.

### Settings And Provider Work

- Switch all LLM roles to Ollama Cloud models.
- Switch the primary agent roles from the default model to
  `ollama:glm-5.1:cloud`, send a live chat turn, then switch back to a local or
  alternate cloud model and verify the visible model labels and runtime calls
  stay aligned.
- Switch only one role at a time, especially agent initial, agent thinking,
  memory compilation, wiki, categorization, and embeddings, to catch accidental
  shared-provider coupling.
- Load provider credentials from environment without displaying secrets.
- Validate authenticated local Ollama Cloud with no raw `OLLAMA_API_KEY` in the
  TUI projection, and validate explicit env-key loading when
  `OLLAMA_API_KEY`/`OPENROUTER_API_KEY` is set before launch.
- Set OpenRouter, Ollama, and ElevenLabs metadata without raw keys.
- Set STT provider and key-presence values.
- Set ElevenLabs STT/TTS and voice choices.
- Set or clear local model hints.
- Explain effective provider fallback when required keys are missing.
- Keep model IDs and display names coherent after edits.
- Confirm provider credentials and model choices persist after quitting and
  relaunching the TUI from the same data directory.
- Confirm provider/model errors are surfaced in chat/status while queue,
  bookmarks, clips, and memory navigation remain responsive.

### Error, Recovery, And Trust Work

- Handle missing `OLLAMA_API_KEY`.
- Handle a provider rejecting credentials.
- Handle no library content.
- Handle no selected episode.
- Handle no transcript.
- Handle no identity for notes/comments/social.
- Handle network failures during search, RSS ingest, or relay access.
- Avoid claiming success before the projection reflects the action.
- Recover from Esc/cancel during input.
- Preserve keyboard navigation after errors.

## Minimum Live Tmux Scenarios For This Pass

1. Configure Ollama Cloud across all model rows through the TUI Settings tab,
   load env credentials, and verify the visible projection reflects the edits.
2. Subscribe to a real RSS feed through the TUI, open the Agent tab, ask the
   agent to recommend an episode from the new library, and inspect the response.
3. Ask the agent to remember a preference, verify Memory projection updates,
   then ask for a recommendation that should use that preference.
4. Create, enable/disable, run-now, and delete an agent task from the Agent tab.
5. Ask the agent to queue or play a selected recommendation, then verify Queue
   or Player projection changes.
6. Exercise a missing-prerequisite path, such as publishing an agent note
   without identity or summarizing an episode without a transcript, and verify
   the TUI reports the limitation clearly.

## Architecture Regression Pass After Provider/Task PRs

Use this pass after the shared provider transport and typed task intent branches
land. It is meant to catch UX regressions caused by moving model/provider calls
and task creation into shared backend paths.

### Tmux Harness

Run the real TUI with an isolated data directory:

```sh
tests/integration/run_tui_glm_tmux_validation.sh
```

The helper verifies `tmux`, `ollama`, and `glm-5.1:cloud` availability, builds
`podcast-tui`, launches a tmux session, and writes an initial pane capture. It
does not send fake agent responses or dispatch kernel calls directly.

Default session details:

- Session: `podcast-tui-glm-architecture`
- Data directory: `/tmp/podcast-tui-glm-architecture-data`
- Capture directory: `/tmp/podcast-tui-glm-architecture-captures`
- Model: `glm-5.1:cloud`

Capture evidence after every scenario:

```sh
tmux capture-pane -t podcast-tui-glm-architecture -p -S - \
  > /tmp/podcast-tui-glm-architecture-captures/NN-scenario-name.txt
```

### Scenario Script

1. Launch with the helper above and capture the first screen.
2. In Settings > Providers, set all LLM role rows to
   `ollama:glm-5.1:cloud | GLM 5.1 Cloud` and set Ollama chat URL to
   `http://localhost:11434/api/chat`; capture the projected settings.
3. Quit and relaunch using the same data directory; verify the provider/model
   rows persisted and still show no raw secrets.
4. Send a one-sentence Agent chat prompt that explicitly asks for GLM Cloud
   confirmation; verify a live assistant response appears and no stale busy
   state remains.
5. Switch only the agent initial model to an alternate model, then back to
   `ollama:glm-5.1:cloud`; send another chat turn and verify display labels and
   behavior follow the selected model.
6. Save memory through Agent > Memory, then ask for a recommendation that must
   use that fact; capture the Memory row and chat response.
7. Subscribe to `https://feeds.npr.org/510289/podcast.xml`; star an episode,
   open Bookmarks, queue it, add it next, play it, and remove the bookmark.
   Capture Library, Bookmarks, Queue, and Player projections.
8. Queue an episode from Library, remove it in Queue, clear Queue, then ask the
   agent to queue a relevant recommendation; verify the queue changes only when
   the backend confirms the action.
9. Create a clip with no episode playing and verify the limitation is reported
   clearly; then play an episode, create a clip, open Clips, play/delete it, and
   capture the projection changes.
10. Create typed tasks for the post-architecture task categories exposed by the
    TUI: memory write, inbox/library triage, queue/playback, bookmark, clip,
    note/social, and wiki/summary where supported. For each task, verify create,
    disable, blocked run-now, enable, run-now, completion/error status, and
    delete.
11. Create a task that writes memory, run it, and verify the memory projection
    updates from the real task execution.
12. Exercise missing-prerequisite paths: provider key missing/rejected,
    no selected episode, no transcript, no identity for notes/social, and no
    library content. Verify the TUI reports limitations without terminal log
    floods or stuck navigation.
13. While an agent call is in flight, switch tabs through Queue, Bookmarks,
    Clips, Memory, and Settings; verify animation/navigation remains smooth and
    no panel overlaps or stale busy rows persist after completion.
14. Quit and relaunch from the same data directory; verify subscribed library,
    queue, bookmarks, clips, memory, provider settings, and tasks match the
    expected persistence behavior.

### Coordination Notes

- Do not edit provider transport or task intent parser files from this
  validation branch; report failures against the owning architecture branches.
- Treat any raw namespace/body task editor that remains after the typed-task PR
  lands as a validation failure unless the PR deliberately preserves it behind
  an explicit compatibility label.
- A provider failure is acceptable only when it is visible to the user as chat
  or status text and the TUI remains navigable. Silent fallback, fabricated
  action success, or terminal log flooding should block merge.
- For authenticated local Ollama Cloud, the TUI may show Ollama credentials as
  `none`; this is valid when direct `ollama run glm-5.1:cloud` and `/api/chat`
  calls succeed through the local daemon.
- Do not rely on the model to self-attest which model handled a prompt. Treat
  Settings projection, captured configured model rows, and live completion
  evidence as the source of truth.

## Evidence Log

### 2026-06-07T12:09Z-12:13Z

- Worktree: `/Users/customer/podcast-player-tui-architecture-validation`
- Branch: `feat/tui-architecture-validation`
- Tmux session: `podcast-tui-arch-smoke`
- Data directory: `/tmp/podcast-tui-arch-smoke-data`
- Capture directory: `/tmp/podcast-tui-arch-smoke-captures`
- Local Ollama:
  - `ollama ls` showed `glm-5.1:cloud`.
  - The tmux helper verified tmux/Ollama/model availability before launch.

### Scenarios Executed

- Tmux helper:
  - Built `podcast-tui` and launched the real TUI in tmux with isolated state.
  - Captured the empty-library first screen at
    `/tmp/podcast-tui-arch-smoke-captures/00-launch.txt`.
- Settings > Providers:
  - Used only TUI keystrokes to set Agent initial and Agent thinking rows to
    `GLM 5.1 Cloud (ollama:glm-5.1:cloud)`.
  - Set Ollama chat URL to `http://localhost:11434/api/chat`.
  - Verified the projection still showed Ollama credential as `none`, matching
    authenticated local Ollama Cloud behavior.
- Agent chat, no-tool turn:
  - Sent through tmux:
    `In one short sentence, confirm this architecture validation smoke is using GLM 5.1 cloud.`
  - Verified the real assistant row completed and the busy indicator cleared.
  - The response declined to self-attest model identity:
    `I have no information indicating what specific model or architecture this validation smoke is using.`
    This is not a transport failure, but future smoke wording should avoid
    depending on model self-identification.
- Agent memory + memory-aware chat:
  - Saved `validation_topic=terminal GLM architecture smoke` from Agent >
    Memory and verified the Memory projection showed it.
  - Asked through chat:
    `Based on saved memory, what validation topic should you remember? Answer in one short sentence.`
  - Verified the assistant row completed with:
    `The validation topic I should remember is terminal GLM architecture smoke.`
- Cleanup:
  - Quit the tmux session with `q`; no `podcast-tui-arch-smoke` tmux session
    remained.

### Failures Or Watch Items

- Current main smoke did not exercise the pending shared-provider/task-intent
  architecture because those branches had not landed yet.
- Model self-identification prompts are weak validation evidence; use captured
  Settings rows plus live completion behavior instead.

### 2026-06-07T09:34Z-10:12Z

- Worktree: `/Users/customer/podcast-player-tui-agent-validation`
- Branch: `feat/tui-agent-live-validation`
- Tmux session: `podcast-tui-agent-live`
- Data directories:
  - `/tmp/podcast-tui-agent-live`
  - `/tmp/podcast-tui-agent-live-feed`
- Environment:
  - `OLLAMA_API_KEY` was not set.
  - `OPENROUTER_API_KEY` was not set.
  - Full live Ollama Cloud completion scenarios are blocked until a real
    `OLLAMA_API_KEY` is present in the terminal environment before launching
    the TUI.

### Scenarios Executed

- Settings > Providers > Load provider keys from env:
  - Before fix, the action could claim env credentials loaded even when no
    provider env keys existed.
  - After fix, the TUI reports:
    `no provider env keys set; set OLLAMA_API_KEY, OPENROUTER_API_KEY, ELEVENLABS_API_KEY, or ASSEMBLYAI_API_KEY`.
- Settings > Providers model rows:
  - Used only TUI keystrokes to set all visible role rows to
    `ollama:gpt-oss:120b-cloud | GPT-OSS 120B Cloud`.
  - Verified the Settings projection displayed the Ollama Cloud selection for
    agent initial/thinking, memory, wiki, categorization, chapter compilation,
    embeddings, and image generation.
- Agent chat, empty library:
  - Sent: `Recommend one podcast episode for me from my current library and explain why.`
  - Before fix, the TUI stayed pinned at `Agent Chat busy` for more than a
    minute.
  - After fix, the assistant row reported the missing Ollama Cloud credential
    in about four seconds.
- Agent memory:
  - Entered `preferred_duration=short episodes under 30 minutes` from the
    Agent > Memory section.
  - Verified the Memory projection showed
    `preferred_duration = short episodes under 30 minutes (user)`.
- Agent tasks:
  - Created `Refresh inbox now | manual | podcast.inbox | {"op":"triage"} | manual validation task`.
  - Disabled it through `e`, verified row changed to `off`.
  - Pressed `r` while disabled; before fix it still reported `task dispatched`.
  - After fix it reports `task run error: task disabled`.
  - Re-enabled, ran, and deleted the task; projection returned to the built-in
    Inbox Triage row.
- Real RSS subscription:
  - Subscribed to `https://feeds.npr.org/510289/podcast.xml` through the TUI.
  - Verified `Planet Money` loaded with 355 episodes.
  - Before fix, background picks/categorization LLM calls wrote repeated 401
    unauthorized logs into the terminal UI.
  - After fix, the subscription completed cleanly with no 401 log flood and
    the TUI remained readable.
- Agent chat, populated library:
  - Sent: `From my current library, recommend one episode to start with and give a one-sentence reason.`
  - Verified the assistant row reported the missing `OLLAMA_API_KEY` instead
    of fabricating a recommendation.

### Fixes From Live TUI Use

- Provider env load now errors when no relevant env keys are set.
- `Ctrl+U` clears settings and relay input fields, making model replacement
  practical in the terminal.
- Provider-prefixed `ollama:` and `openrouter:` role selections are now honored
  by the shared LLM factory instead of falling back to hardcoded defaults.
- Ollama backend strips the `ollama:` provider prefix before calling the model.
- Ollama Cloud and OpenRouter paths preflight required in-memory keys before
  starting model calls.
- Agent model turns have a 45-second wall-clock budget.
- Production agent chat writes concrete provider errors into the assistant row
  instead of falling back to the scaffold placeholder.
- Picks now shares the visible Categorization model row; episode summaries
  share the visible Wiki model row, avoiding hidden hardcoded model choices.
- Background missing-credential failures fall back quietly instead of dumping
  per-episode logs into the TUI.
- TUI task run checks the selected task's enabled state before dispatching, so
  disabled tasks cannot claim they were dispatched.
- TUI dispatch parsing now recognizes nested `{ok:false}` result envelopes when
  they are present.

### 2026-06-07T10:32Z-11:23Z

- Worktree: `/Users/customer/podcast-player-tui-agent-validation`
- Branch: `feat/tui-agent-live-validation`
- Tmux session: `podcast-tui-glm-live`
- Data directory: `/tmp/podcast-tui-glm-live`
- Local Ollama:
  - `ollama ls` showed `glm-5.1:cloud` plus other `:cloud` models.
  - Direct `ollama run glm-5.1:cloud` returned the requested
    `tui-live-ok` smoke response.
  - Direct `/api/chat` calls confirmed `stream:false` + `think:false` works
    against the local daemon.

### Scenarios Executed

- Settings > Providers:
  - Verified the live TUI projection showed all visible LLM role rows as
    `GLM 5.1 Cloud (ollama:glm-5.1:cloud)`.
  - Verified `Ollama chat URL` showed `http://localhost:11434/api/chat`.
  - Verified OpenRouter/Ollama credential rows remained `none`, matching the
    authenticated local Ollama Cloud path rather than env-key injection.
- Agent chat, no-tool turn:
  - Sent through tmux: `In one short sentence, confirm this live TUI agent
    model call succeeded using GLM 5.1 cloud.`
  - The assistant row completed with:
    `This live TUI agent model call using GLM 5.1 cloud has succeeded.`
- Agent memory + memory-aware chat:
  - Entered `favorite_topic=ancient history podcasts` in Agent > Memory.
  - Verified the Memory projection showed the saved fact.
  - Asked through chat what topic should be prioritized based on memory.
  - The assistant row completed with:
    `You should prioritize ancient history podcasts for me.`
- Agent tool loop, empty library:
  - Sent through tmux: `Use your library search tool to look for economics
    podcasts in my library, then tell me what you found.`
  - The assistant row completed with:
    `I searched your library for economics podcasts, but no matches were found.`
- Agent tasks:
  - Created `Live GLM Memory Task | once | podcast.memory |
    {"op":"remember","key":"task_live","value":"ran","source":"user"} |
    writes memory through live TUI`.
  - Disabled it, attempted a run, re-enabled it, and ran it.
  - Verified the task row reached `completed` and Memory showed
    `task_live = ran (user)`.

### Additional Fixes From GLM Live TUI Use

- Replaced the Rig Ollama chat path with direct `/api/chat` requests so local
  Ollama Cloud models complete reliably from the app backend.
- Normalized `http://localhost:*` Ollama URLs to `http://127.0.0.1:*` before
  request dispatch to avoid IPv6-first localhost connection failures.
- Added a direct async agent chat path so the production handler no longer
  nests a Tokio runtime inside a spawned blocking task.
- Shortened tool instructions so GLM cloud models reliably emit tool-call JSON
  and final prose within the TUI request budget.
- Added a TUI snapshot-revision probe on the 250 ms tick so host-side async
  completions, including agent replies and task-memory writes, refresh the
  visible terminal projection even when no new NMP actor callback arrives.
