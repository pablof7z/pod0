import Pod0Core

extension Pod0NativeHostDispatcher {
    func startNotificationTask(
        _ envelope: HostRequestEnvelope,
        occurrenceID: FeedDiscoveryOccurrenceId,
        episodeID: EpisodeId,
        podcastID: PodcastId,
        podcastTitle: String,
        episodeTitle: String,
        delivery: @escaping Delivery
    ) {
        let task = Task { @MainActor [weak self] in
            guard let self else { return }
            let result = await notificationHost.deliver(
                occurrenceID: occurrenceID,
                episodeID: episodeID,
                podcastID: podcastID,
                podcastTitle: podcastTitle,
                episodeTitle: episodeTitle
            )
            guard activeTasks.removeValue(forKey: envelope.requestId) != nil else { return }
            let observation: HostObservation = isExpired(envelope)
                ? .failed(code: .timedOut, safeDetail: "Host request deadline expired")
                : result
            finish(
                envelope,
                sequenceNumber: 0,
                observation: observation,
                delivery: delivery
            )
        }
        activeTasks[envelope.requestId] = ActiveTask(
            envelope: envelope,
            task: task,
            delivery: delivery
        )
    }

    func cancelNotificationIfNeeded(_ request: HostRequest) {
        guard case let .deliverNewEpisodeNotification(
            occurrenceID,
            _,
            _,
            _,
            _
        ) = request else { return }
        notificationHost.cancel(occurrenceID: occurrenceID)
    }
}
