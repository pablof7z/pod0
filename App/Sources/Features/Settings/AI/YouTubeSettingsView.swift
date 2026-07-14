import SwiftUI

struct YouTubeSettingsView: View {
    @Environment(AppStateStore.self) private var store
    @State private var urlInput: String = ""

    var body: some View {
        Form {
            endpointSection
        }
        .listStyle(.insetGrouped)
        .navigationTitle("YouTube Ingestion")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .navigationBarTrailing) {
                Button("Save") { commitURL() }
                    .disabled(urlInput.trimmed == (store.state.settings.youtubeExtractorURL ?? ""))
            }
        }
        .onAppear {
            urlInput = store.state.settings.youtubeExtractorURL ?? ""
        }
    }

    private var endpointSection: some View {
        Section {
            TextField("https://your-extractor.example.com", text: $urlInput)
                .keyboardType(.URL)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .onSubmit { commitURL() }

            if !urlInput.isBlank, URL(string: urlInput.trimmed) == nil {
                Text("Enter a valid URL.")
                    .inlineErrorText()
            }

            if store.state.settings.youtubeExtractorURL != nil {
                Button(role: .destructive) {
                    urlInput = ""
                    var s = store.state.settings
                    s.youtubeExtractorURL = nil
                    store.updateSettings(s)
                } label: {
                    Label("Remove Endpoint", systemImage: "trash")
                }
            }
        } header: {
            Text("Extractor Endpoint")
        } footer: {
            Text("Point to a self-hosted YouTube audio extractor (e.g. cobalt, yt-dlp wrapper). The agent POSTs {\"url\": \"<youtube_url>\"} and expects back JSON with an \"audio_url\" field plus optional \"title\", \"author\", and \"duration_seconds\".")
        }
    }

    private func commitURL() {
        let trimmed = urlInput.trimmed
        var s = store.state.settings
        s.youtubeExtractorURL = trimmed.isEmpty ? nil : trimmed
        store.updateSettings(s)
        urlInput = trimmed
    }
}
