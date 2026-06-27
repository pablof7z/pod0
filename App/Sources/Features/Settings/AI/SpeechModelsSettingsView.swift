import SwiftUI

struct SpeechModelsSettingsView: View {
    @Environment(AppStateStore.self) private var store

    @State private var ttsPreview = ElevenLabsTTSPreviewService()
    @State private var isTestingVoice = false
    @State private var testVoiceError: String?
    @State private var speechCatalog = SpeechModelCatalog()
    @State private var speechCatalogError: String?

    var body: some View {
        Form {
            speechToTextSection
            textToSpeechSection
            voiceSection
        }
        .navigationTitle("Speech")
        .navigationBarTitleDisplayMode(.inline)
        .task {
            await loadSpeechCatalog()
        }
        .animation(AppTheme.Animation.spring, value: testVoiceError)
        .animation(AppTheme.Animation.spring, value: speechCatalogError)
    }

    // MARK: - Sections

    private var speechToTextSection: some View {
        Section {
            Picker(selection: sttProviderBinding) {
                ForEach(STTProvider.allCases, id: \.self) { provider in
                    Text(provider.displayName).tag(provider)
                }
            } label: {
                Label("Provider", systemImage: "waveform.badge.mic")
            }
            .pickerStyle(.menu)

            if currentSettings.sttProvider == .elevenLabsScribe {
                Picker(selection: elevenLabsSTTModelBinding) {
                    ForEach(speechCatalog.elevenLabsSTT, id: \.id) { entry in
                        Text(entry.label).tag(entry.id)
                    }
                    customModelEntry(
                        currentID: currentSettings.elevenLabsSTTModel,
                        knownIDs: speechCatalog.elevenLabsSTT.map(\.id)
                    )
                } label: {
                    Label("Model", systemImage: "cpu")
                }
                .pickerStyle(.menu)
            }

            if currentSettings.sttProvider == .openRouterWhisper {
                Picker(selection: openRouterWhisperModelBinding) {
                    ForEach(speechCatalog.openRouterWhisper, id: \.id) { entry in
                        Text(entry.label).tag(entry.id)
                    }
                    customModelEntry(
                        currentID: currentSettings.openRouterWhisperModel,
                        knownIDs: speechCatalog.openRouterWhisper.map(\.id)
                    )
                } label: {
                    Label("Model", systemImage: "cpu")
                }
                .pickerStyle(.menu)
            }

            if currentSettings.sttProvider == .assemblyAI {
                Picker(selection: assemblyAISTTModelBinding) {
                    ForEach(speechCatalog.assemblyAISTT, id: \.id) { entry in
                        Text(entry.label).tag(entry.id)
                    }
                    customModelEntry(
                        currentID: currentSettings.assemblyAISTTModel,
                        knownIDs: speechCatalog.assemblyAISTT.map(\.id)
                    )
                } label: {
                    Label("Model", systemImage: "cpu")
                }
                .pickerStyle(.menu)
            }

            if let speechCatalogError {
                Text(speechCatalogError)
                    .inlineErrorText()
            }
        } header: {
            Text("Transcription")
        } footer: {
            transcriptionFooter
        }
    }

    private var transcriptionFooter: Text {
        switch currentSettings.sttProvider {
        case .elevenLabsScribe:
            return Text("ElevenLabs Scribe — diarization and word-level timestamps. Requires an ElevenLabs key.")
        case .assemblyAI:
            return Text("AssemblyAI - speaker labels and word timestamps. Requires an AssemblyAI key.")
        case .openRouterWhisper:
            return Text("OpenRouter Whisper — uses your OpenRouter key. Downloaded episodes are uploaded for transcription.")
        case .appleNative:
            return Text("Apple on-device — uses Apple Silicon's neural engine via iOS 26 SpeechTranscriber. No API key required. Episode must be downloaded first.")
        }
    }

    private var textToSpeechSection: some View {
        Section {
            Picker(selection: elevenLabsTTSModelBinding) {
                ForEach(speechCatalog.elevenLabsTTS, id: \.id) { entry in
                    Text(entry.label).tag(entry.id)
                }
                customModelEntry(
                    currentID: currentSettings.elevenLabsTTSModel,
                    knownIDs: speechCatalog.elevenLabsTTS.map(\.id)
                )
            } label: {
                Label("Text to Speech", systemImage: "speaker.wave.2.fill")
            }
            .pickerStyle(.menu)
        } header: {
            Text("Narration")
        } footer: {
            Text("Used for spoken agent picks and voice previews. AssemblyAI is transcription-only, so narration still uses ElevenLabs.")
        }
    }

    private var voiceSection: some View {
        Section {
            NavigationLink {
                ElevenLabsVoiceBrowserView()
            } label: {
                SettingsRow(
                    icon: "waveform.and.mic",
                    tint: AppTheme.Brand.elevenLabsTint,
                    title: "Voice",
                    value: voiceDisplayName
                )
            }

            Button {
                Task { await testVoice() }
            } label: {
                HStack {
                    if isTestingVoice {
                        Label("Speaking...", systemImage: "waveform")
                            .symbolEffect(.variableColor.iterative, isActive: isTestingVoice)
                    } else {
                        Label("Test Voice", systemImage: "speaker.wave.2")
                    }
                    Spacer()
                }
            }
            .disabled(isTestingVoice || store.state.settings.elevenLabsVoiceID.isEmpty || !hasElevenLabsKey)
            .tint(AppTheme.Brand.elevenLabsTint)

            if let testVoiceError {
                Text(testVoiceError)
                    .inlineErrorText()
                    .transition(.opacity.combined(with: .move(edge: .top)))
            }
        } header: {
            Text("Voice")
        } footer: {
            Text("Connect ElevenLabs in Providers before testing speech output.")
        }
    }

    // MARK: - Helpers

    private var currentSettings: Settings {
        store.state.settings
    }

    private var sttProviderBinding: Binding<STTProvider> {
        Binding(
            get: { currentSettings.sttProvider },
            set: { updateSettings(\.sttProvider, to: $0) }
        )
    }

    private var elevenLabsSTTModelBinding: Binding<String> {
        Binding(
            get: { currentSettings.elevenLabsSTTModel },
            set: { updateSettings(\.elevenLabsSTTModel, to: $0) }
        )
    }

    private var openRouterWhisperModelBinding: Binding<String> {
        Binding(
            get: { currentSettings.openRouterWhisperModel },
            set: { updateSettings(\.openRouterWhisperModel, to: $0) }
        )
    }

    private var assemblyAISTTModelBinding: Binding<String> {
        Binding(
            get: { currentSettings.assemblyAISTTModel },
            set: { updateSettings(\.assemblyAISTTModel, to: $0) }
        )
    }

    private var elevenLabsTTSModelBinding: Binding<String> {
        Binding(
            get: { currentSettings.elevenLabsTTSModel },
            set: { updateSettings(\.elevenLabsTTSModel, to: $0) }
        )
    }

    private func updateSettings<Value>(_ keyPath: WritableKeyPath<Settings, Value>, to value: Value) {
        var next = currentSettings
        next[keyPath: keyPath] = value
        store.updateSettings(next)
    }

    @ViewBuilder
    private func customModelEntry(currentID: String, knownIDs: [String]) -> some View {
        if !currentID.isBlank && !knownIDs.contains(currentID) {
            Text("\(currentID) (custom)").tag(currentID)
        }
    }

    private var hasElevenLabsKey: Bool {
        (store.kernel?.settings ?? SettingsSnapshot()).elevenLabsKeyPresent
    }

    private var voiceDisplayName: String {
        let current = store.state.settings
        guard !current.elevenLabsVoiceID.isBlank else { return "Not set" }
        return current.elevenLabsVoiceName.isBlank ? "Selected" : current.elevenLabsVoiceName
    }

    private func loadSpeechCatalog() async {
        do {
            speechCatalog = try await SpeechModelCatalogService().fetchCatalog()
            speechCatalogError = nil
        } catch {
            speechCatalogError = error.localizedDescription
        }
    }

    private func testVoice() async {
        testVoiceError = nil
        isTestingVoice = true
        defer { isTestingVoice = false }
        do {
            let current = store.state.settings
            try await ttsPreview.speak(
                voiceID: current.elevenLabsVoiceID,
                model: current.elevenLabsTTSModel
            )
        } catch {
            testVoiceError = error.localizedDescription
        }
    }
}
