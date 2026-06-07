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

### 2026-06-07T20:10Z-20:27Z

- Worktree: `/Users/customer/podcast-player-tui-glm-post-architecture-validation`
- Branch: `codex/tui-glm-post-architecture-validation`
- Tmux sessions:
  - `podcast-tui-glm-post`
  - `podcast-tui-glm-post-fixed`
- Data directory: `/tmp/podcast-tui-glm-post-data`
- Capture directories:
  - `/tmp/podcast-tui-glm-post-captures`
  - `/tmp/podcast-tui-glm-post-fixed-captures`
- Local Ollama:
  - `ollama ls` showed `glm-5.1:cloud`.
  - The helper verified `tmux`, `ollama`, and the cloud model before launch.

### Scenarios Executed

- Settings > Providers:
  - Used real TUI input to set visible model roles to
    `GLM 5.1 Cloud (ollama:glm-5.1:cloud)`.
  - Verified the Ollama URL projection as `http://localhost:11434/api/chat`.
  - Verified credentials remained hidden/`none`, matching authenticated local
    Ollama Cloud.
- Agent chat:
  - Sent a live provider-backed chat prompt through tmux.
  - Verified the GLM response completed and the busy state cleared.
- Memory:
  - Saved `validation_topic=shared TUI GLM post architecture`.
  - Asked a follow-up chat turn that depended on saved memory and verified the
    answer included the saved value.
- Typed task editor:
  - Created a task through the typed/natural TUI form rather than raw
    namespace/body JSON.
  - Verified the row showed user-facing intent detail.
  - Verified disabled run-now reported `task run error: task disabled`.
  - Enabled and ran the task, then verified Memory projected
    `task_validation = completed (task)`.
  - Deleted the validation task and verified only the built-in Inbox Triage
    task remained.
- Library, queue, and bookmarks:
  - Subscribed to `https://feeds.npr.org/510289/podcast.xml` through the TUI.
  - Verified `Planet Money` projected with 355 episodes.
  - Starred an episode, queued it, played it through the TUI fallback player,
    and verified Queue and Stars projections after relaunch.
- Persistence:
  - Relaunched from the same data directory and verified library, queue,
    bookmarks, provider settings, and memory/task projections persisted.
- Missing prerequisite:
  - Published an Agent Note without an active account and verified the TUI
    reported `agent note error: not signed in`.

### Fixes From This Run

- Removed stdout/stderr terminal writes from TUI audio fallback and unknown
  capability handling. Playback state still updates through returned JSON
  envelopes, but the alternate screen is no longer corrupted by log text.
- Added a TUI Agent Notes prerequisite check so no-identity publish attempts
  surface immediately as `agent note error: not signed in`. The backend
  remains the source of truth for actual publish authorization.
- No NMP upstream issue was filed from this run; observed failures were TUI
  presentation/status handling, not missing shared NMP architecture seams.

### Earlier 2026-06-07 Runs

- `feat/tui-agent-live-validation` used tmux sessions
  `podcast-tui-agent-live` and `podcast-tui-glm-live` to validate provider env
  loading, visible role-model selection, missing-key errors, memory, task
  disable/run/delete, real RSS subscription, direct Ollama `/api/chat`, and
  GLM-backed library search behavior.
- Fixes from those runs included honest provider-env errors, provider-prefixed
  model routing through the shared LLM factory, Ollama prefix stripping,
  direct Ollama chat transport, localhost normalization, provider-error chat
  rows, quiet background missing-credential failures, disabled task run
  blocking, nested `{ok:false}` dispatch parsing, and snapshot-revision polling
  for async host completions.
- `feat/tui-architecture-validation` used `podcast-tui-arch-smoke` to verify
  the tmux helper, authenticated local Ollama Cloud availability, basic
  Settings projection, live GLM completion, and memory-aware chat before the
  shared provider/task branches had landed.
- Watch item retained: do not rely on model self-identification as evidence.
  Use captured Settings rows plus live completion/projection behavior instead.
