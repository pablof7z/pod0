import SwiftUI

struct ElevenLabsVoiceBrowserView: View {

    private enum Layout {
        static let toolbarItemSpacing: CGFloat = 12
    }

    @Environment(AppStateStore.self) private var store
    @Environment(\.dismiss) private var dismiss

    @State private var viewModel = ElevenLabsVoiceBrowserViewModel()
    @State private var searchText = ""
    @State private var genderFilter: String?
    @State private var accentFilter: String?

    var body: some View {
        Group {
            switch viewModel.phase {
            case .needsAPIKey:
                ElevenLabsVoiceBrowserMissingKeyView { dismiss() }
            case .loading where viewModel.voices.isEmpty:
                ElevenLabsVoiceBrowserLoadingView()
            case .error(let message) where viewModel.voices.isEmpty:
                ElevenLabsVoiceBrowserErrorView(message: message) {
                    Task { await viewModel.reload() }
                }
            default:
                voicesList
            }
        }
        .navigationTitle("Voice")
        .navigationBarTitleDisplayMode(.inline)
        .searchable(text: $searchText, prompt: "Search voices")
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                HStack(spacing: Layout.toolbarItemSpacing) {
                    filterMenu
                    Button {
                        Task { await viewModel.reload() }
                    } label: {
                        Image(systemName: "arrow.clockwise")
                    }
                    .disabled(viewModel.phase == .loading)
                    .accessibilityLabel("Refresh voices")
                }
            }
        }
        .task {
            await viewModel.loadIfNeeded()
        }
        .onDisappear {
            viewModel.stopPreview()
        }
    }

    // MARK: - Filter menu

    private var filterMenu: some View {
        Menu {
            genderPicker
            if !availableAccents.isEmpty {
                accentPicker
            }
            if isFiltering {
                Divider()
                Button(role: .destructive) {
                    genderFilter = nil
                    accentFilter = nil
                } label: {
                    Label("Clear Filters", systemImage: "xmark.circle")
                }
            }
        } label: {
            Image(systemName: isFiltering ? "line.3.horizontal.decrease.circle.fill" : "line.3.horizontal.decrease.circle")
                .symbolRenderingMode(.hierarchical)
                .foregroundStyle(isFiltering ? Color.accentColor : .secondary)
        }
        .accessibilityLabel("Filter voices")
        .disabled(viewModel.voices.isEmpty)
    }

    @ViewBuilder
    private var genderPicker: some View {
        if !availableGenders.isEmpty {
            Picker("Gender", selection: $genderFilter) {
                Text("Any Gender").tag(String?.none)
                ForEach(availableGenders, id: \.self) { gender in
                    Text(gender).tag(Optional(gender))
                }
            }
            .pickerStyle(.inline)
        }
    }

    @ViewBuilder
    private var accentPicker: some View {
        Picker("Accent", selection: $accentFilter) {
            Text("Any Accent").tag(String?.none)
            ForEach(availableAccents, id: \.self) { accent in
                Text(accent).tag(Optional(accent))
            }
        }
        .pickerStyle(.inline)
    }

    // MARK: - Voices list

    private var voicesList: some View {
        List {
            if case .error(let message) = viewModel.phase {
                Section {
                    Label(message, systemImage: "exclamationmark.triangle")
                        .font(AppTheme.Typography.subheadline)
                        .foregroundStyle(AppTheme.Tint.warning)
                }
            }

            currentSection

            if isFiltering {
                activeFilterBanner
            }

            ForEach(filteredGroups, id: \.category) { group in
                Section(ElevenLabsVoiceCategoryOrder.display(group.category)) {
                    ForEach(group.voices) { voice in
                        rowButton(for: voice)
                    }
                }
            }

            if filteredGroups.isEmpty && viewModel.phase != .loading {
                Section {
                    Text("No voices match this search.")
                        .foregroundStyle(.secondary)
                }
            }
        }
        .listStyle(.insetGrouped)
        .refreshable { await viewModel.reload() }
    }

    /// Pins the currently selected voice at the top of the list,
    /// mirroring the pattern used by `OpenRouterModelSelectorView`.
    @ViewBuilder
    private var currentSection: some View {
        let selectedID = store.state.settings.elevenLabsVoiceID
        if !selectedID.isEmpty,
           let current = viewModel.voices.first(where: { $0.voiceID == selectedID }),
           searchText.isEmpty && !isFiltering {
            Section("Current") {
                rowButton(for: current)
            }
        }
    }

    private var activeFilterBanner: some View {
        Section {
            HStack(spacing: AppTheme.Spacing.sm) {
                Image(systemName: "line.3.horizontal.decrease.circle.fill")
                    .foregroundStyle(Color.accentColor)
                Text(activeFilterDescription)
                    .font(AppTheme.Typography.subheadline)
                Spacer()
                Button {
                    genderFilter = nil
                    accentFilter = nil
                } label: {
                    Text("Clear")
                        .font(AppTheme.Typography.subheadline.weight(.medium))
                        .foregroundStyle(Color.accentColor)
                }
                .buttonStyle(.plain)
            }
        }
    }

    private func rowButton(for voice: ElevenLabsVoice) -> some View {
        Button {
            select(voice)
        } label: {
            ElevenLabsVoiceRow(
                voice: voice,
                isSelected: voice.voiceID == store.state.settings.elevenLabsVoiceID,
                isPlaying: viewModel.playingVoiceID == voice.voiceID,
                isLoadingPreview: viewModel.loadingPreviewVoiceID == voice.voiceID,
                canPreview: voice.previewURL != nil,
                onTogglePreview: { viewModel.togglePreview(for: voice) }
            )
        }
        .buttonStyle(.plain)
    }

    // MARK: - Filter helpers

    private var isFiltering: Bool { genderFilter != nil || accentFilter != nil }

    private var activeFilterDescription: String {
        [genderFilter, accentFilter]
            .compactMap { $0 }
            .joined(separator: " · ")
    }

    private var availableGenders: [String] {
        let genders = viewModel.voices.compactMap { $0.gender }.filter { !$0.isEmpty }
        return Array(Set(genders)).map { $0.capitalized }.sorted()
    }

    private var availableAccents: [String] {
        let accents = viewModel.voices.compactMap { $0.accent }.filter { !$0.isEmpty }
        return Array(Set(accents)).map { $0.capitalized }.sorted()
    }

    private var filteredGroups: [ElevenLabsVoiceGroup] {
        let terms = searchText.lowercased().split(whereSeparator: \.isWhitespace).map(String.init)
        var filtered = viewModel.voices

        if !terms.isEmpty {
            filtered = filtered.filter { voice in
                terms.allSatisfy { voice.searchText.contains($0) }
            }
        }
        if let genderFilter {
            filtered = filtered.filter {
                $0.gender?.caseInsensitiveCompare(genderFilter) == .orderedSame
            }
        }
        if let accentFilter {
            filtered = filtered.filter {
                $0.accent?.caseInsensitiveCompare(accentFilter) == .orderedSame
            }
        }

        let grouped = Dictionary(grouping: filtered, by: \.category)
        return grouped
            .map { ElevenLabsVoiceGroup(category: $0.key, voices: $0.value.sorted { $0.name.localizedCaseInsensitiveCompare($1.name) == .orderedAscending }) }
            .sorted { lhs, rhs in
                let l = ElevenLabsVoiceCategoryOrder.sortKey(lhs.category)
                let r = ElevenLabsVoiceCategoryOrder.sortKey(rhs.category)
                if l != r { return l < r }
                return lhs.category.localizedCaseInsensitiveCompare(rhs.category) == .orderedAscending
            }
    }

    // MARK: - Selection

    private func select(_ voice: ElevenLabsVoice) {
        var settings = store.state.settings
        guard settings.elevenLabsVoiceID != voice.voiceID else { return }
        settings.elevenLabsVoiceID = voice.voiceID
        settings.elevenLabsVoiceName = voice.name
        store.updateSettings(settings)
        Haptics.success()
    }
}

