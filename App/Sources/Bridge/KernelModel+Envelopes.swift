import Foundation

// MARK: - KernelModel envelope accessors
//
// Read-only wrappers that forward to the `PodcastHandle` FFI envelope calls.
// Each method serialises its arguments and returns an opaque JSON string (or
// `nil` on error) that the caller decodes into its own view-model type.
// Extracted from KernelModel.swift to keep that file under the AGENTS.md
// 500-line hard limit.

extension KernelModel {

    func itunesDirectorySearchEnvelope(query: String, type: String, limit: Int) -> String? {
        kernel.itunesDirectorySearchEnvelope(query: query, type: type, limit: limit)
    }

    func itunesLookupFeedEnvelope(collectionID: String) -> String? {
        kernel.itunesLookupFeedEnvelope(collectionID: collectionID)
    }

    func itunesTopPodcastsEnvelope(limit: Int, storefront: String) -> String? {
        kernel.itunesTopPodcastsEnvelope(limit: limit, storefront: storefront)
    }

    func threadingProjectionEnvelope() -> String? {
        kernel.threadingProjectionEnvelope()
    }

    func threadingActiveTopicsEnvelope(limit: Int, podcastIDs: [UUID]) -> String? {
        kernel.threadingActiveTopicsEnvelope(limit: limit, podcastIDs: podcastIDs)
    }

    func agentInventoryEnvelope(request: [String: Any]) -> String? {
        kernel.agentInventoryEnvelope(request: request)
    }

    func agentEmptyStateEnvelope() -> String? {
        kernel.agentEmptyStateEnvelope()
    }

    func localSearchEnvelope(query: String, limit: Int) -> String? {
        kernel.localSearchEnvelope(query: query, limit: limit)
    }

    func homeContinueListeningEnvelope(limit: Int, podcastIDs: [UUID]) -> String? {
        kernel.homeContinueListeningEnvelope(limit: limit, podcastIDs: podcastIDs)
    }

    func homeTriageRollupEnvelope(podcastIDs: [UUID]) -> String? {
        kernel.homeTriageRollupEnvelope(podcastIDs: podcastIDs)
    }

    func homeSubscriptionListEnvelope(filter: String, podcastIDs: [UUID]) -> String? {
        kernel.homeSubscriptionListEnvelope(filter: filter, podcastIDs: podcastIDs)
    }

    func carplayListenNowEnvelope(limit: Int) -> String? {
        kernel.carplayListenNowEnvelope(limit: limit)
    }

    func carplayShowsEnvelope(limit: Int) -> String? {
        kernel.carplayShowsEnvelope(limit: limit)
    }

    func carplayShowEpisodesEnvelope(podcastID: UUID, limit: Int) -> String? {
        kernel.carplayShowEpisodesEnvelope(podcastID: podcastID, limit: limit)
    }

    func carplayDownloadsEnvelope(limit: Int) -> String? {
        kernel.carplayDownloadsEnvelope(limit: limit)
    }

    func libraryShowEpisodesEnvelope(podcastID: UUID, limit: Int) -> String? {
        kernel.libraryShowEpisodesEnvelope(podcastID: podcastID, limit: limit)
    }

    func libraryPodcastStatsEnvelope(podcastIDs: [UUID]) -> String? {
        kernel.libraryPodcastStatsEnvelope(podcastIDs: podcastIDs)
    }

    func libraryEpisodeForAudioURLEnvelope(audioURL: String, podcastID: UUID) -> String? {
        kernel.libraryEpisodeForAudioURLEnvelope(audioURL: audioURL, podcastID: podcastID)
    }

    func librarySummaryEnvelope() -> String? {
        kernel.librarySummaryEnvelope()
    }

    func libraryAllEpisodesEnvelope(filter: String, query: String, limit: Int) -> String? {
        kernel.libraryAllEpisodesEnvelope(filter: filter, query: query, limit: limit)
    }

    func libraryAllPodcastsEnvelope(query: String) -> String? {
        kernel.libraryAllPodcastsEnvelope(query: query)
    }

    func libraryFollowedPodcastsEnvelope() -> String? {
        kernel.libraryFollowedPodcastsEnvelope()
    }

    func libraryOwnedPodcastsEnvelope() -> String? {
        kernel.libraryOwnedPodcastsEnvelope()
    }

    func libraryCategoriesEnvelope(categories: [[String: Any]]) -> String? {
        kernel.libraryCategoriesEnvelope(categories: categories)
    }

    func libraryDownloadRowsEnvelope() -> String? {
        kernel.libraryDownloadRowsEnvelope()
    }

    func libraryStarredEpisodesEnvelope() -> String? {
        kernel.libraryStarredEpisodesEnvelope()
    }

    func libraryEpisodeLookupEnvelope(reference: String) -> String? {
        kernel.libraryEpisodeLookupEnvelope(reference: reference)
    }

    func librarySubscriptionStatusEnvelope(feedURL: String?, ownerPubkey: String?, podcastID: String? = nil) -> String? {
        kernel.librarySubscriptionStatusEnvelope(feedURL: feedURL, ownerPubkey: ownerPubkey, podcastID: podcastID)
    }

    func libraryPodcastForOwnerPubkeyEnvelope(ownerPubkey: String) -> String? {
        kernel.libraryPodcastForOwnerPubkeyEnvelope(ownerPubkey: ownerPubkey)
    }

    func libraryCategorizationPromptEnvelope() -> String? {
        kernel.libraryCategorizationPromptEnvelope()
    }

    func libraryCategorizationParseEnvelope(rawContent: String) -> String? {
        kernel.libraryCategorizationParseEnvelope(rawContent: rawContent)
    }

    func agentChatTitlePromptEnvelope(messages: [[String: String]]) -> String? {
        kernel.agentChatTitlePromptEnvelope(messages: messages)
    }

    func agentChatTitleParseEnvelope(rawContent: String) -> String? {
        kernel.agentChatTitleParseEnvelope(rawContent: rawContent)
    }

    func agentNostrPeerPromptEnvelope(
        peerPubkey: String,
        peerDisplayName: String?,
        peerAbout: String?,
        ownerPubkey: String?
    ) -> String? {
        kernel.agentNostrPeerPromptEnvelope(
            peerPubkey: peerPubkey,
            peerDisplayName: peerDisplayName,
            peerAbout: peerAbout,
            ownerPubkey: ownerPubkey
        )
    }

    func agentSystemPromptEnvelope(request: [String: Any]) -> String? {
        kernel.agentSystemPromptEnvelope(request: request)
    }

    func agentConversationHistoryEnvelope(request: [String: Any]) -> String? {
        kernel.agentConversationHistoryEnvelope(request: request)
    }

    func libraryCategoryChangeEnvelope(request: [String: Any]) -> String? {
        kernel.libraryCategoryChangeEnvelope(request: request)
    }

    func homeCategoryCardsEnvelope(categories: [[String: Any]]) -> String? {
        kernel.homeCategoryCardsEnvelope(categories: categories)
    }

    func storageBreakdownEnvelope(files: [[String: Any]]) -> String? {
        kernel.storageBreakdownEnvelope(files: files)
    }

    func agentTTSEpisodePlanEnvelope(request: [String: Any]) -> String? {
        kernel.agentTTSEpisodePlanEnvelope(request: request)
    }

    func agentTTSDefaultVoiceEnvelope() -> String? {
        kernel.agentTTSDefaultVoiceEnvelope()
    }

    func agentGeneratedPodcastDescriptorEnvelope() -> String? {
        kernel.agentGeneratedPodcastDescriptorEnvelope()
    }
}
