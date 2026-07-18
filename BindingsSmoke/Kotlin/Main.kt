import uniffi.pod0_application.ApplicationCommand
import uniffi.pod0_application.CoreFailureCode
import uniffi.pod0_application.OperationStage
import uniffi.pod0_application.Projection
import uniffi.pod0_application.ProjectionEnvelope
import uniffi.pod0_application.ProjectionRequest
import uniffi.pod0_application.ProjectionScope
import uniffi.pod0_domain.CancellationId
import uniffi.pod0_domain.CommandId
import uniffi.pod0_facade.Pod0Facade
import uniffi.pod0_facade.ProjectionSubscriber
import java.io.File

private class RecordingSubscriber : ProjectionSubscriber {
    val revisions = mutableListOf<ULong>()

    override fun receive(projection: ProjectionEnvelope) {
        revisions.add(projection.stateRevision.value)
    }
}

fun main(args: Array<String>) {
    val fixture = decodeProperties(File(args.single()).readText())
    check(fixture["fixture_version"] == "1")
    check(fixture["schema_component"] == "kernel")
    check(fixture["stored_version"]?.toUInt() == 2u)
    check(fixture["supported_min"]?.toUInt() == 0u)
    check(fixture["supported_max"]?.toUInt() == 3u)
    check(fixture["access_mode"] == "migration_only")
    check(fixture["migration_state"] == "required")
    check(fixture["target_version"]?.toUInt() == 3u)
    check(fixture["store_id_high"]?.toULong() == 10UL)
    check(fixture["store_id_low"]?.toULong() == 11UL)
    check(fixture["command_id_high"]?.toULong() == 1UL)
    check(fixture["command_id_low"]?.toULong() == 2UL)
    check(fixture["state_revision"]?.toULong() == 42UL)
    check(fixture["operation_stage"] == "failed")
    check(fixture["error_kind"] == "unsupported")
    check(fixture["error_wire_code"]?.toUInt() == 9001u)
    check(fixture["optional_safe_detail"] == "null")

    val facade = Pod0Facade()
    try {
        val subscriber = RecordingSubscriber()
        val request = ProjectionRequest(ProjectionScope.Library, 20u)
        val handle = facade.subscribe(request, subscriber)
        check(subscriber.revisions == listOf(0UL))

        facade.dispatch(
            uniffi.pod0_application.CommandEnvelope(
                CommandId(0UL, 1UL),
                CancellationId(0UL, 2UL),
                null,
                ApplicationCommand.Unsupported(77u),
            ),
        )
        check(subscriber.revisions == listOf(0UL, 1UL))

        val projection = facade.snapshot(request).projection
        check(projection is Projection.Library)
        val unsupportedOperation = projection.value.operations.single()
        check(unsupportedOperation.commandId == CommandId(0UL, 1UL))
        check(unsupportedOperation.cancellationId == CancellationId(0UL, 2UL))
        check(unsupportedOperation.stage is OperationStage.Failed)
        val unsupportedFailure = unsupportedOperation.failure
        check(unsupportedFailure?.code == CoreFailureCode.Unsupported(77u))
        check(unsupportedFailure.safeDetail == null)

        facade.dispatch(
            uniffi.pod0_application.CommandEnvelope(
                CommandId(0UL, 3UL),
                CancellationId(0UL, 4UL),
                null,
                ApplicationCommand.SubscribeToFeed("https://example.test/feed"),
            ),
        )
        facade.dispatch(
            uniffi.pod0_application.CommandEnvelope(
                CommandId(0UL, 5UL),
                CancellationId(0UL, 6UL),
                null,
                ApplicationCommand.CancelOperation(CancellationId(0UL, 4UL)),
            ),
        )
        check(facade.nextHostRequests(64u.toUShort()).isEmpty())
        val cancelledProjection = facade.snapshot(request).projection
        check(cancelledProjection is Projection.Library)
        check(cancelledProjection.value.operations.any { operation ->
            operation.commandId == CommandId(0UL, 3UL) &&
                operation.stage is OperationStage.Cancelled &&
                operation.failure?.code is CoreFailureCode.Cancelled
        })

        facade.unsubscribe(handle)
        facade.dispatch(
            uniffi.pod0_application.CommandEnvelope(
                CommandId(0UL, 7UL),
                CancellationId(0UL, 8UL),
                null,
                ApplicationCommand.Unsupported(78u),
            ),
        )
        check(subscriber.revisions == listOf(0UL, 1UL, 2UL, 3UL))
    } finally {
        facade.destroy()
    }
}

private fun decodeProperties(text: String): Map<String, String> =
    text.lineSequence()
        .filter { line -> line.isNotEmpty() && !line.startsWith("#") }
        .associate { line ->
            val separator = line.indexOf('=')
            check(separator > 0)
            line.substring(0, separator) to line.substring(separator + 1)
        }
