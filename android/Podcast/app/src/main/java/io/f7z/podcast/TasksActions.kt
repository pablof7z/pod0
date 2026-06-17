package io.f7z.podcast

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

/**
 * Canonical wire contract for the `podcast.tasks` kernel action namespace.
 *
 * Wire shapes verified against
 * `apps/nmp-app-podcast/src/ffi/actions/tasks_module.rs`
 * (`AgentTasksModule::NAMESPACE = "podcast.tasks"`):
 *
 *  `create_from_intent` — `{"op":"create_from_intent","title":"…","schedule":"…",
 *                           "intent":{"type":"agent_prompt","prompt":"…"}}`
 *                         Mints a UUID server-side; returns `{"ok":true,"task_id":"<uuid>"}`.
 *  `update_from_intent` — `{"op":"update_from_intent","task_id":"…","title":"…",
 *                           "schedule":"…","intent":{"type":"agent_prompt","prompt":"…"}}`
 *  `delete`             — `{"op":"delete","task_id":"…"}`
 *  `enable`             — `{"op":"enable","task_id":"…"}`
 *  `disable`            — `{"op":"disable","task_id":"…"}`
 *  `run_now`            — `{"op":"run_now","task_id":"…"}`
 *
 * The Rust enum uses `#[serde(tag = "op", rename_all = "snake_case")]`, so op
 * values are snake_case variant names: `CreateFromIntent` → `"create_from_intent"`.
 *
 * `AgentTaskIntent` uses `#[serde(tag = "type", rename_all = "snake_case")]`, so
 * the nested intent object is `{"type":"agent_prompt","prompt":"…"}`.
 *
 * **All field names must be spelled exactly in snake_case** — Android has NO
 * automatic field-name conversion, unlike the iOS bridge. Wrong casing silently
 * drops the field (dispatch trap documented in ActionDispatcher.kt).
 *
 * Payload builders are pure functions — no [KernelBridge] dependency — so they
 * can be tested without the native library loaded (same pattern as [ClipActions]).
 */
object TasksActions {

    /**
     * Action namespace.
     * Source of truth: `AgentTasksModule::NAMESPACE` in
     * `apps/nmp-app-podcast/src/ffi/actions/tasks_module.rs`.
     */
    const val NAMESPACE = "podcast.tasks"

    private val json = Json

    // ── Public dispatch helpers ──────────────────────────────────────────────

    /**
     * Dispatch `podcast.tasks` `create_from_intent` with an `agent_prompt` intent.
     *
     * The kernel mints a UUID for the new task and returns
     * `{"ok":true,"task_id":"<uuid>"}`. The next snapshot push includes the
     * new task in `agent_tasks`.
     */
    fun createPromptTask(
        bridge: KernelBridge,
        title: String,
        prompt: String,
        schedule: String,
    ): String? = bridge.dispatchAction(NAMESPACE, buildCreateFromIntentPayload(title, prompt, schedule))

    /**
     * Dispatch `podcast.tasks` `update_from_intent` for an `agent_prompt` task.
     *
     * Replaces the task's title, schedule, and prompt in the kernel store.
     * The next snapshot push reflects the updated fields.
     */
    fun updatePromptTask(
        bridge: KernelBridge,
        taskId: String,
        title: String,
        prompt: String,
        schedule: String,
    ): String? = bridge.dispatchAction(NAMESPACE, buildUpdateFromIntentPayload(taskId, title, prompt, schedule))

    /** Dispatch `podcast.tasks` `delete` — removes the task permanently. */
    fun delete(bridge: KernelBridge, taskId: String): String? =
        bridge.dispatchAction(NAMESPACE, buildDeletePayload(taskId))

    /** Dispatch `podcast.tasks` `enable` — allows the scheduler to fire the task. */
    fun enable(bridge: KernelBridge, taskId: String): String? =
        bridge.dispatchAction(NAMESPACE, buildEnablePayload(taskId))

    /** Dispatch `podcast.tasks` `disable` — suspends the task without deleting it. */
    fun disable(bridge: KernelBridge, taskId: String): String? =
        bridge.dispatchAction(NAMESPACE, buildDisablePayload(taskId))

    /**
     * Dispatch `podcast.tasks` `run_now` — triggers the task immediately regardless
     * of its schedule or enabled state.
     */
    fun runNow(bridge: KernelBridge, taskId: String): String? =
        bridge.dispatchAction(NAMESPACE, buildRunNowPayload(taskId))

    // ── Pure payload builders (testable without bridge) ──────────────────────

    /**
     * Build the `create_from_intent` wire payload for an `agent_prompt` task.
     *
     * Rust contract (`AgentTasksAction::CreateFromIntent` + `AgentTaskIntent::AgentPrompt`):
     * ```json
     * {"op":"create_from_intent","title":"…","schedule":"…",
     *  "intent":{"type":"agent_prompt","prompt":"…"}}
     * ```
     * Valid [schedule] values accepted by the Rust parser:
     * `"hourly"`, `"daily"`, `"weekly"`, `"once"`, `"every <N>s"`.
     */
    fun buildCreateFromIntentPayload(title: String, prompt: String, schedule: String): String =
        json.encodeToString(
            JsonObject.serializer(),
            JsonObject(
                mapOf(
                    "op"       to JsonPrimitive("create_from_intent"),
                    "title"    to JsonPrimitive(title),
                    "schedule" to JsonPrimitive(schedule),
                    "intent"   to JsonObject(
                        mapOf(
                            "type"   to JsonPrimitive("agent_prompt"),
                            "prompt" to JsonPrimitive(prompt),
                        ),
                    ),
                ),
            ),
        )

    /**
     * Build the `update_from_intent` wire payload for an `agent_prompt` task.
     *
     * Rust contract (`AgentTasksAction::UpdateFromIntent` + `AgentTaskIntent::AgentPrompt`):
     * ```json
     * {"op":"update_from_intent","task_id":"…","title":"…","schedule":"…",
     *  "intent":{"type":"agent_prompt","prompt":"…"}}
     * ```
     */
    fun buildUpdateFromIntentPayload(
        taskId: String,
        title: String,
        prompt: String,
        schedule: String,
    ): String =
        json.encodeToString(
            JsonObject.serializer(),
            JsonObject(
                mapOf(
                    "op"       to JsonPrimitive("update_from_intent"),
                    "task_id"  to JsonPrimitive(taskId),
                    "title"    to JsonPrimitive(title),
                    "schedule" to JsonPrimitive(schedule),
                    "intent"   to JsonObject(
                        mapOf(
                            "type"   to JsonPrimitive("agent_prompt"),
                            "prompt" to JsonPrimitive(prompt),
                        ),
                    ),
                ),
            ),
        )

    /**
     * Build the `delete` wire payload.
     *
     * Rust contract (`AgentTasksAction::Delete`):
     * ```json
     * {"op":"delete","task_id":"<uuid>"}
     * ```
     */
    fun buildDeletePayload(taskId: String): String =
        json.encodeToString(
            JsonObject.serializer(),
            JsonObject(
                mapOf(
                    "op"      to JsonPrimitive("delete"),
                    "task_id" to JsonPrimitive(taskId),
                ),
            ),
        )

    /**
     * Build the `enable` wire payload.
     *
     * Rust contract (`AgentTasksAction::Enable`):
     * ```json
     * {"op":"enable","task_id":"<uuid>"}
     * ```
     */
    fun buildEnablePayload(taskId: String): String =
        json.encodeToString(
            JsonObject.serializer(),
            JsonObject(
                mapOf(
                    "op"      to JsonPrimitive("enable"),
                    "task_id" to JsonPrimitive(taskId),
                ),
            ),
        )

    /**
     * Build the `disable` wire payload.
     *
     * Rust contract (`AgentTasksAction::Disable`):
     * ```json
     * {"op":"disable","task_id":"<uuid>"}
     * ```
     */
    fun buildDisablePayload(taskId: String): String =
        json.encodeToString(
            JsonObject.serializer(),
            JsonObject(
                mapOf(
                    "op"      to JsonPrimitive("disable"),
                    "task_id" to JsonPrimitive(taskId),
                ),
            ),
        )

    /**
     * Build the `run_now` wire payload.
     *
     * Rust contract (`AgentTasksAction::RunNow`):
     * ```json
     * {"op":"run_now","task_id":"<uuid>"}
     * ```
     */
    fun buildRunNowPayload(taskId: String): String =
        json.encodeToString(
            JsonObject.serializer(),
            JsonObject(
                mapOf(
                    "op"      to JsonPrimitive("run_now"),
                    "task_id" to JsonPrimitive(taskId),
                ),
            ),
        )
}
