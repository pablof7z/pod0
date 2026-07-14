import XCTest
@testable import Podcastr

/// Covers the agent skills system: registry shape, schema gating, dispatcher
/// gate, prompt rendering, and `ChatConversation` round-trip with the new
/// `enabledSkills` field.
@MainActor
final class AgentSkillsTests: XCTestCase {

    // MARK: - Registry shape

    func testRegistryListsPodcastGenerationSkill() {
        XCTAssertNotNil(AgentSkillRegistry.skill(id: AgentSkillID.podcastGeneration))
    }

    func testRegistryListsWikiResearchSkill() {
        XCTAssertNotNil(AgentSkillRegistry.skill(id: AgentSkillID.wikiResearch))
    }

    func testRegistryUnknownSkillReturnsNil() {
        XCTAssertNil(AgentSkillRegistry.skill(id: "does_not_exist"))
    }

    func testSchemasForEmptySetIsEmpty() {
        XCTAssertTrue(AgentSkillRegistry.schemas(for: []).isEmpty)
    }

    func testSchemasForPodcastGenerationReturnsThreeTools() {
        let schemas = AgentSkillRegistry.schemas(for: [AgentSkillID.podcastGeneration])
        let names = Set(schemas.compactMap { ($0["function"] as? [String: Any])?["name"] as? String })
        XCTAssertEqual(names, [
            AgentTools.PodcastNames.generateTTSEpisode,
            AgentTools.PodcastNames.configureAgentVoice,
            AgentTools.PodcastNames.listAvailableVoices,
        ])
    }

    func testSchemasForWikiResearchReturnsThreeTools() {
        let schemas = AgentSkillRegistry.schemas(for: [AgentSkillID.wikiResearch])
        let names = Set(schemas.compactMap { ($0["function"] as? [String: Any])?["name"] as? String })
        XCTAssertEqual(names, [
            AgentTools.PodcastNames.createWikiPage,
            AgentTools.PodcastNames.listWikiPages,
            AgentTools.PodcastNames.deleteWikiPage,
        ])
    }

    func testQueryWikiStaysAlwaysOn() {
        // query_wiki is a cheap lookup — it must NOT be skill-gated.
        XCTAssertNil(AgentSkillRegistry.owningSkillID(forTool: AgentTools.PodcastNames.queryWiki))
        let podcastNames = Set(AgentTools.podcastSchema.compactMap {
            ($0["function"] as? [String: Any])?["name"] as? String
        })
        XCTAssertTrue(podcastNames.contains(AgentTools.PodcastNames.queryWiki))
    }

    func testOwningSkillLookup() {
        XCTAssertEqual(
            AgentSkillRegistry.owningSkillID(forTool: AgentTools.PodcastNames.generateTTSEpisode),
            AgentSkillID.podcastGeneration
        )
        XCTAssertEqual(
            AgentSkillRegistry.owningSkillID(forTool: AgentTools.PodcastNames.listAvailableVoices),
            AgentSkillID.podcastGeneration
        )
        XCTAssertEqual(
            AgentSkillRegistry.owningSkillID(forTool: AgentTools.PodcastNames.createWikiPage),
            AgentSkillID.wikiResearch
        )
        XCTAssertEqual(
            AgentSkillRegistry.owningSkillID(forTool: AgentTools.PodcastNames.deleteWikiPage),
            AgentSkillID.wikiResearch
        )
        // Non-skill-gated podcast tools — no owner.
        XCTAssertNil(AgentSkillRegistry.owningSkillID(forTool: AgentTools.PodcastNames.playEpisode))
        XCTAssertNil(AgentSkillRegistry.owningSkillID(forTool: AgentTools.PodcastNames.queryWiki))
    }

    func testAllToolNamesCoversEverySkill() {
        for skill in AgentSkillRegistry.all {
            for name in skill.toolNames {
                XCTAssertTrue(
                    AgentSkillRegistry.allToolNames.contains(name),
                    "allToolNames missing \(name) from skill \(skill.id)"
                )
            }
        }
    }

    func testSkillToolNamesAreAllRoutedByDispatchPodcast() {
        // Every skill-gated tool must be in PodcastNames.all so dispatch can
        // route to dispatchPodcast (the only place that knows how to handle
        // them). Without this, the skill-enabled happy path would 404.
        let routed = Set(AgentTools.PodcastNames.all)
        for skill in AgentSkillRegistry.all {
            for name in skill.toolNames {
                XCTAssertTrue(routed.contains(name), "\(name) missing from PodcastNames.all")
            }
        }
    }

    // MARK: - Schema OpenAI shape

    func testSkillSchemasHaveOpenAIFunctionShape() {
        for entry in AgentSkillRegistry.schemas(for: [AgentSkillID.podcastGeneration]) {
            XCTAssertEqual(entry["type"] as? String, "function")
            let function = entry["function"] as? [String: Any]
            XCTAssertNotNil(function?["name"] as? String)
            XCTAssertNotNil(function?["description"] as? String)
            let params = function?["parameters"] as? [String: Any]
            XCTAssertEqual(params?["type"] as? String, "object")
            XCTAssertNotNil(params?["properties"] as? [String: Any])
            XCTAssertNotNil(params?["required"] as? [String])
        }
    }

    // MARK: - Dispatcher gate

    func testDispatchPodcastBlocksSkillToolWhenSkillNotEnabled() async throws {
        let deps = makeDeps()
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.generateTTSEpisode,
            args: ["title": "x", "turns": [["kind": "speech", "text": "hi"]] as [Any]],
            deps: deps,
            enabledSkills: []
        )
        let decoded = try decode(json)
        XCTAssertNotNil(decoded["error"])
        let err = decoded["error"] as? String ?? ""
        XCTAssertTrue(err.contains("podcast_generation"), "Error must mention the missing skill, got: \(err)")
    }

    func testDispatchPodcastAllowsSkillToolWhenSkillEnabled() async throws {
        let deps = makeDeps()
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.generateTTSEpisode,
            args: ["title": "x", "turns": [["kind": "speech", "text": "hi"]] as [Any]],
            deps: deps,
            enabledSkills: [AgentSkillID.podcastGeneration]
        )
        let decoded = try decode(json)
        // MockTTSPublisher publishes successfully — should not be a skill-gate error.
        XCTAssertNil(decoded["error"])
        XCTAssertEqual(decoded["success"] as? Bool, true)
    }

    func testDispatchPodcastBlocksConfigureAgentVoiceWhenSkillOff() async throws {
        let deps = makeDeps()
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.configureAgentVoice,
            args: ["voice_id": "v123"],
            deps: deps,
            enabledSkills: []
        )
        let decoded = try decode(json)
        XCTAssertNotNil(decoded["error"])
    }

    func testDispatchPodcastBlocksWikiToolWhenWikiSkillOff() async throws {
        let deps = makeDeps()
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.listWikiPages,
            args: [:],
            deps: deps,
            enabledSkills: []
        )
        let decoded = try decode(json)
        XCTAssertNotNil(decoded["error"])
        let err = decoded["error"] as? String ?? ""
        XCTAssertTrue(err.contains("wiki_research"), "Error must mention the missing skill, got: \(err)")
    }

    func testDispatchPodcastAllowsWikiToolWhenWikiSkillEnabled() async throws {
        let deps = makeDeps()
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.listWikiPages,
            args: [:],
            deps: deps,
            enabledSkills: [AgentSkillID.wikiResearch]
        )
        let decoded = try decode(json)
        XCTAssertNil(decoded["error"])
    }

    func testDispatchPodcastQueryWikiAlwaysOn() async throws {
        // query_wiki must work without any skill — it's the cheap read path.
        let deps = makeDeps()
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.queryWiki,
            args: ["topic": "anything"],
            deps: deps,
            enabledSkills: []
        )
        let decoded = try decode(json)
        XCTAssertNil(decoded["error"])
    }

    // MARK: - Prompt catalog

    func testSystemPromptContainsSkillsSection() {
        let prompt = AgentPrompt.build(for: AppState())
        XCTAssertTrue(prompt.contains("## Skills"))
        XCTAssertTrue(prompt.contains(AgentSkillID.podcastGeneration))
        XCTAssertTrue(prompt.contains("use_skill"))
    }

    // MARK: - use_skill tool schema

    func testUseSkillToolIsInBaseSchema() {
        let names = Set(AgentTools.schema.compactMap { tool -> String? in
            (tool["function"] as? [String: Any])?["name"] as? String
        })
        XCTAssertTrue(names.contains(AgentTools.Names.useSkill))
    }

    // MARK: - Activation flow (use_skill)

    func testActivateUnlocksSkillAndReturnsManual() throws {
        let result = AgentSkillRegistry.activate(
            argsJSON: #"{"skill_id":"\#(AgentSkillID.podcastGeneration)"}"#,
            currentEnabledSkills: []
        )
        XCTAssertTrue(result.updatedEnabledSkills.contains(AgentSkillID.podcastGeneration))
        let decoded = try decode(result.resultJSON)
        XCTAssertEqual(decoded["success"] as? Bool, true)
        XCTAssertEqual(decoded["skill_id"] as? String, AgentSkillID.podcastGeneration)
        XCTAssertEqual(decoded["already_enabled"] as? Bool, false)
        let manual = decoded["manual"] as? String ?? ""
        XCTAssertFalse(manual.isEmpty, "First activation must include the manual")
        let unlocked = decoded["tools_unlocked"] as? [String] ?? []
        XCTAssertTrue(unlocked.contains(AgentTools.PodcastNames.generateTTSEpisode))
        XCTAssertTrue(unlocked.contains(AgentTools.PodcastNames.listAvailableVoices))
    }

    func testActivateUnknownSkillReturnsError() throws {
        let result = AgentSkillRegistry.activate(
            argsJSON: #"{"skill_id":"does_not_exist"}"#,
            currentEnabledSkills: []
        )
        XCTAssertTrue(result.updatedEnabledSkills.isEmpty)
        let decoded = try decode(result.resultJSON)
        XCTAssertNotNil(decoded["error"])
    }

    func testActivateMissingSkillIDReturnsError() throws {
        let result = AgentSkillRegistry.activate(
            argsJSON: "{}",
            currentEnabledSkills: []
        )
        XCTAssertTrue(result.updatedEnabledSkills.isEmpty)
        let decoded = try decode(result.resultJSON)
        XCTAssertNotNil(decoded["error"])
    }

    func testActivateMalformedJSONReturnsError() throws {
        let result = AgentSkillRegistry.activate(
            argsJSON: "not json",
            currentEnabledSkills: []
        )
        XCTAssertTrue(result.updatedEnabledSkills.isEmpty)
        let decoded = try decode(result.resultJSON)
        XCTAssertNotNil(decoded["error"])
    }

    func testActivateIdempotentSkipsManualOnReactivation() throws {
        let result = AgentSkillRegistry.activate(
            argsJSON: #"{"skill_id":"\#(AgentSkillID.podcastGeneration)"}"#,
            currentEnabledSkills: [AgentSkillID.podcastGeneration]
        )
        // Still enabled — no toggle, no removal.
        XCTAssertTrue(result.updatedEnabledSkills.contains(AgentSkillID.podcastGeneration))
        let decoded = try decode(result.resultJSON)
        XCTAssertEqual(decoded["already_enabled"] as? Bool, true)
        // Re-activation skips the manual to save context tokens.
        XCTAssertNil(decoded["manual"])
    }

    // MARK: - ChatConversation round-trip

    func testChatConversationPersistsEnabledSkills() throws {
        let convo = ChatConversation(
            id: UUID(),
            title: "x",
            messages: [],
            isUpgraded: false,
            enabledSkills: [AgentSkillID.podcastGeneration],
            createdAt: Date(),
            updatedAt: Date()
        )
        let data = try JSONEncoder().encode(convo)
        let decoded = try JSONDecoder().decode(ChatConversation.self, from: data)
        XCTAssertEqual(decoded.enabledSkills, [AgentSkillID.podcastGeneration])
    }

    func testChatConversationDecodesLegacySnapshotWithoutEnabledSkills() throws {
        let legacy = """
        {"id":"\(UUID().uuidString)","title":"x","messages":[],"isUpgraded":false,"createdAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-01T00:00:00Z"}
        """
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        let decoded = try decoder.decode(ChatConversation.self, from: Data(legacy.utf8))
        XCTAssertEqual(decoded.enabledSkills, [])
    }

    // MARK: - Helpers

    private func decode(_ json: String) throws -> [String: Any] {
        let raw = try JSONSerialization.jsonObject(with: Data(json.utf8))
        guard let obj = raw as? [String: Any] else {
            throw NSError(domain: "test", code: 1)
        }
        return obj
    }

    private func makeDeps() -> PodcastAgentToolDeps {
        PodcastAgentToolDeps(
            rag: MockRAG(),
            wiki: MockWiki(),
            briefing: MockBriefing(),
            summarizer: MockSummarizer(),
            fetcher: MockFetcher(),
            playback: MockPlayback(),
            library: MockLibrary(),
            inventory: MockInventory(),
            categories: MockInventory(),
            peerPublisher: MockPeerEventPublisher(),
            friendDirectory: MockFriendDirectory(),
            perplexity: MockPerplexity(),
            ttsPublisher: MockTTSPublisher(),
            directory: MockDirectory(),
            subscribe: MockSubscribe(),
            youtubeIngestion: MockYouTubeIngestion(),
            ownedPodcasts: MockOwnedPodcasts()
        )
    }
}
