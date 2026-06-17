package io.f7z.podcast

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Unit tests for [TasksActions] wire-payload builders.
 *
 * Asserts the exact JSON shapes expected by
 * `apps/nmp-app-podcast/src/ffi/actions/tasks_module.rs::AgentTasksAction`:
 *
 * ```rust
 * #[serde(tag = "op", rename_all = "snake_case")]
 * pub enum AgentTasksAction {
 *     CreateFromIntent { title, description?, intent, schedule },
 *     UpdateFromIntent { task_id, title, description?, intent, schedule },
 *     Delete { task_id },
 *     Enable { task_id },
 *     Disable { task_id },
 *     RunNow { task_id },
 *     RunDue,
 * }
 *
 * #[serde(tag = "type", rename_all = "snake_case")]
 * pub enum AgentTaskIntent {
 *     InboxTriage,
 *     ClearAgent,
 *     RememberMemory { key, value },
 *     AgentPrompt { prompt },
 * }
 * ```
 *
 * All tests are pure-Kotlin (no Android runtime, no KernelBridge).
 */
class TasksActionsTest {

    private val json = Json { ignoreUnknownKeys = true }

    private fun parse(payload: String): JsonObject =
        json.parseToJsonElement(payload).jsonObject

    // ── NAMESPACE constant ─────────────────────────────────────────────────────

    @Test
    fun `NAMESPACE constant matches Rust AgentTasksModule NAMESPACE`() {
        assertEquals(
            "NAMESPACE must match Rust AgentTasksModule::NAMESPACE = \"podcast.tasks\"",
            "podcast.tasks",
            TasksActions.NAMESPACE,
        )
    }

    // ── create_from_intent ─────────────────────────────────────────────────────

    @Test
    fun `buildCreateFromIntentPayload op field is 'create_from_intent'`() {
        val obj = parse(TasksActions.buildCreateFromIntentPayload("My task", "Do something", "daily"))
        assertEquals(
            "op must be 'create_from_intent' (Rust AgentTasksAction::CreateFromIntent rename_all=snake_case)",
            "create_from_intent",
            obj["op"]?.jsonPrimitive?.content,
        )
    }

    @Test
    fun `buildCreateFromIntentPayload encodes title`() {
        val obj = parse(TasksActions.buildCreateFromIntentPayload("Weekly summary", "Summarize", "weekly"))
        assertEquals("Weekly summary", obj["title"]?.jsonPrimitive?.content)
    }

    @Test
    fun `buildCreateFromIntentPayload encodes schedule`() {
        val obj = parse(TasksActions.buildCreateFromIntentPayload("Task", "Prompt", "hourly"))
        assertEquals("hourly", obj["schedule"]?.jsonPrimitive?.content)
    }

    @Test
    fun `buildCreateFromIntentPayload encodes custom schedule`() {
        val obj = parse(TasksActions.buildCreateFromIntentPayload("Task", "Prompt", "every 3600s"))
        assertEquals("every 3600s", obj["schedule"]?.jsonPrimitive?.content)
    }

    @Test
    fun `buildCreateFromIntentPayload encodes intent as nested object`() {
        val obj = parse(TasksActions.buildCreateFromIntentPayload("Task", "My prompt", "daily"))
        val intent = obj["intent"]?.jsonObject
        assertNotNull("intent must be a nested JSON object", intent)
    }

    @Test
    fun `buildCreateFromIntentPayload intent type is 'agent_prompt'`() {
        val obj = parse(TasksActions.buildCreateFromIntentPayload("Task", "My prompt", "daily"))
        val intentType = obj["intent"]?.jsonObject?.get("type")?.jsonPrimitive?.content
        assertEquals(
            "intent.type must be 'agent_prompt' (Rust AgentTaskIntent::AgentPrompt rename_all=snake_case)",
            "agent_prompt",
            intentType,
        )
    }

    @Test
    fun `buildCreateFromIntentPayload intent prompt field is correct`() {
        val obj = parse(TasksActions.buildCreateFromIntentPayload("Task", "Run inbox triage", "daily"))
        val prompt = obj["intent"]?.jsonObject?.get("prompt")?.jsonPrimitive?.content
        assertEquals("Run inbox triage", prompt)
    }

    @Test
    fun `buildCreateFromIntentPayload produces valid JSON object`() {
        val payload = TasksActions.buildCreateFromIntentPayload("T", "P", "weekly")
        assertTrue(json.parseToJsonElement(payload) is JsonObject)
    }

    // ── update_from_intent ─────────────────────────────────────────────────────

    @Test
    fun `buildUpdateFromIntentPayload op field is 'update_from_intent'`() {
        val obj = parse(TasksActions.buildUpdateFromIntentPayload("uuid-1", "T", "P", "daily"))
        assertEquals(
            "op must be 'update_from_intent'",
            "update_from_intent",
            obj["op"]?.jsonPrimitive?.content,
        )
    }

    @Test
    fun `buildUpdateFromIntentPayload encodes task_id in snake_case`() {
        val obj = parse(TasksActions.buildUpdateFromIntentPayload("task-uuid-42", "T", "P", "hourly"))
        assertEquals(
            "task_id must be snake_case (Rust UpdateFromIntent field name)",
            "task-uuid-42",
            obj["task_id"]?.jsonPrimitive?.content,
        )
    }

    @Test
    fun `buildUpdateFromIntentPayload encodes title and schedule`() {
        val obj = parse(TasksActions.buildUpdateFromIntentPayload("id", "Updated title", "New prompt", "weekly"))
        assertEquals("Updated title", obj["title"]?.jsonPrimitive?.content)
        assertEquals("weekly", obj["schedule"]?.jsonPrimitive?.content)
    }

    @Test
    fun `buildUpdateFromIntentPayload intent type is 'agent_prompt'`() {
        val obj = parse(TasksActions.buildUpdateFromIntentPayload("id", "T", "P", "daily"))
        assertEquals("agent_prompt", obj["intent"]?.jsonObject?.get("type")?.jsonPrimitive?.content)
    }

    @Test
    fun `buildUpdateFromIntentPayload intent prompt is encoded`() {
        val obj = parse(TasksActions.buildUpdateFromIntentPayload("id", "T", "New prompt text", "daily"))
        assertEquals("New prompt text", obj["intent"]?.jsonObject?.get("prompt")?.jsonPrimitive?.content)
    }

    // ── delete ─────────────────────────────────────────────────────────────────

    @Test
    fun `buildDeletePayload op field is 'delete'`() {
        val obj = parse(TasksActions.buildDeletePayload("task-1"))
        assertEquals(
            "op must be 'delete' (Rust AgentTasksAction::Delete rename_all=snake_case)",
            "delete",
            obj["op"]?.jsonPrimitive?.content,
        )
    }

    @Test
    fun `buildDeletePayload encodes task_id in snake_case`() {
        val obj = parse(TasksActions.buildDeletePayload("my-task-uuid"))
        assertEquals(
            "task_id must be snake_case (Rust Delete field name)",
            "my-task-uuid",
            obj["task_id"]?.jsonPrimitive?.content,
        )
    }

    @Test
    fun `buildDeletePayload does not include intent or schedule`() {
        val obj = parse(TasksActions.buildDeletePayload("task-x"))
        assertNull("intent must not be present in delete payload", obj["intent"])
        assertNull("schedule must not be present in delete payload", obj["schedule"])
    }

    // ── enable ─────────────────────────────────────────────────────────────────

    @Test
    fun `buildEnablePayload op field is 'enable'`() {
        val obj = parse(TasksActions.buildEnablePayload("task-1"))
        assertEquals("enable", obj["op"]?.jsonPrimitive?.content)
    }

    @Test
    fun `buildEnablePayload encodes task_id`() {
        val obj = parse(TasksActions.buildEnablePayload("task-enable-me"))
        assertEquals("task-enable-me", obj["task_id"]?.jsonPrimitive?.content)
    }

    // ── disable ────────────────────────────────────────────────────────────────

    @Test
    fun `buildDisablePayload op field is 'disable'`() {
        val obj = parse(TasksActions.buildDisablePayload("task-1"))
        assertEquals("disable", obj["op"]?.jsonPrimitive?.content)
    }

    @Test
    fun `buildDisablePayload encodes task_id`() {
        val obj = parse(TasksActions.buildDisablePayload("task-disable-me"))
        assertEquals("task-disable-me", obj["task_id"]?.jsonPrimitive?.content)
    }

    // ── run_now ────────────────────────────────────────────────────────────────

    @Test
    fun `buildRunNowPayload op field is 'run_now'`() {
        val obj = parse(TasksActions.buildRunNowPayload("task-1"))
        assertEquals(
            "op must be 'run_now' (Rust AgentTasksAction::RunNow rename_all=snake_case)",
            "run_now",
            obj["op"]?.jsonPrimitive?.content,
        )
    }

    @Test
    fun `buildRunNowPayload encodes task_id in snake_case`() {
        val obj = parse(TasksActions.buildRunNowPayload("task-run-now"))
        assertEquals(
            "task_id must be snake_case (Rust RunNow field name)",
            "task-run-now",
            obj["task_id"]?.jsonPrimitive?.content,
        )
    }

    @Test
    fun `buildRunNowPayload does not include schedule or intent`() {
        val obj = parse(TasksActions.buildRunNowPayload("t"))
        assertNull("schedule must not be present in run_now payload", obj["schedule"])
        assertNull("intent must not be present in run_now payload", obj["intent"])
    }
}
