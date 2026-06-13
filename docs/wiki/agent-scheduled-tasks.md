---
title: Agent Scheduled Tasks
slug: agent-scheduled-tasks
topic: agent-system
summary: The agent can schedule recurring prompts at a given cadence (like a cron job) via a tool call
tags:
  - capture
volatility: warm
confidence: medium
created: 2026-05-13
updated: 2026-06-13
verified: 2026-05-13
compiled-from: conversation
sources:
  - session:3b6253ac-ef01-489b-a3dc-a0a5932e8d0a
  - session:c1691db0-d63e-4062-adad-1cfa0d679d09
---

# Agent Scheduled Tasks

## Agent Scheduled Tasks

The agent can schedule recurring prompts at a given cadence (like a cron job) via a tool call. The `schedule_task` tool schema accepts both `interval_seconds` (int) and `cadence` ('hourly'|'daily'|'weekly' enum string). The tools `cancel_scheduled_task` and `list_scheduled_tasks` are also provided in `AgentTools+Schedule.swift`.

When a scheduled time arrives and the app is active, the system creates a new agent conversation running the scheduled prompt as a user-role message. The user prompt from a scheduled task appears in the conversation transcript. If the app was offline or inactive for multiple missed scheduled periods, it runs only once (not multiple catch-up runs).

`AgentScheduledTaskRunner` is a `MainActor` `final` class that takes a store and an optional `podcastDepsProvider` closure. Its `runDueTasksIfNeeded()` method is called on service-ready and on every app foreground. The `podcastDepsProvider` is wired in `RootView.task(id:)` after `PlaybackState` is available, following the `NostrRelayService` pattern.

`whats-new.json` must include an entry for user-facing scheduled agent task changes.

The 2-hour autonomous loop cron (`ec073427`) plans and executes cycles of Fable→Sonnet→Opus→Haiku workflows, merging reviewed PRs to `main` with docs/wiki as the north star. It is session-only and auto-expires after 7 days, and can be stopped with `CronDelete ec073427`.

<!-- citations: [^3b625-1] [^3b625-2] [^3b625-3] [^3b625-4] [^c1691-273] [^c1691-295] -->
## Domain Model

The `AgentScheduledTask` domain model has fields: `id`, `label`, `prompt`, `intervalSeconds`, `nextRunAt`, and an `isDue` computed property. `AppState` includes a persisted `agentScheduledTasks: [AgentScheduledTask]` array with forward-compatible decoding. <!-- [^3b625-5] -->

## Execution and Scheduling Mechanics

Scheduled task run timing uses `nextRunAt = now + intervalSeconds` (not `previousNextRunAt + intervalSeconds`) to avoid chain-firing catch-up runs, and tasks are marked as run BEFORE executing to prevent crash/restart double-fire. `ChatConversation` has an `isScheduledTask: Bool` flag (default `false`, forward-compat decoded) to prevent scheduled-task conversations from hijacking the auto-resume path. `ChatHistoryStore.mostRecent` skips conversations where `isScheduledTask == true`. <!-- [^3b625-6] -->

## Headless Session Configuration

`AgentChatSession.init` accepts a `podcastDeps: PodcastAgentToolDeps?` parameter alongside `playback:`, allowing callers to pass pre-built deps for headless scheduled-task sessions. Headless scheduled-task sessions pass `askCoordinator: nil` to `AgentChatSession` and use `drainPendingContext: false` to prevent scheduled tasks from consuming pending user context. <!-- [^3b625-7] -->

## Run Source Identification

`AgentRunSource` includes a `.scheduledTask` case for distinguishing scheduled-task agent runs in the run log. <!-- [^3b625-8] -->
