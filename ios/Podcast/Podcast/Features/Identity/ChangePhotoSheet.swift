import PhotosUI
import SwiftUI

// MARK: - ChangePhotoSheet
//
// Action-sheet-style chooser per identity-05-synthesis §4.4. Three entries:
// "Choose a style" (curated 6), "Choose from library" (PhotosPicker → Blossom
// upload), and "Paste image URL". Camera capture lands in a later brief.
//
// NMP migration note: the library-upload path requires both a kernel-backed
// signer (M1 exit) and the Blossom capability (M10). Until either lands,
// "Choose from library" surfaces the staged-action banner; "Choose a style"
// and "Paste image URL" remain fully functional because they only mutate
// the local `pictureURL` binding.

struct ChangePhotoSheet: View {

    @Binding var pictureURL: String
    @Environment(\.dismiss) private var dismiss

    @State private var pasteURL: String = ""
    @State private var pasteVisible = false
    @State private var photoItem: PhotosPickerItem?
    @State private var uploadState: UploadState = .idle

    init(pictureURL: Binding<String>) {
        self._pictureURL = pictureURL
    }

    private enum UploadState: Equatable {
        case idle
        case loading
        case failed(String)

        var isLoading: Bool { self == .loading }
    }

    var body: some View {
        NavigationStack {
            List {
                Section {
                    NavigationLink {
                        AvatarStylePickerView(pictureURL: $pictureURL)
                    } label: {
                        Label("Choose a style", systemImage: "circle.grid.3x3.fill")
                    }
                    .disabled(uploadState.isLoading)

                    PhotosPicker(selection: $photoItem, matching: .images, photoLibrary: .shared()) {
                        Label("Choose from library", systemImage: "photo.on.rectangle")
                    }
                    .disabled(uploadState.isLoading)

                    Button {
                        pasteVisible.toggle()
                    } label: {
                        Label("Paste image URL", systemImage: "link")
                    }
                    .disabled(uploadState.isLoading)

                    if pasteVisible {
                        TextField("https://…", text: $pasteURL)
                            .textInputAutocapitalization(.never)
                            .autocorrectionDisabled()
                            .keyboardType(.URL)
                        Button("Use this URL") {
                            pictureURL = pasteURL.trimmed
                            Haptics.success()
                            dismiss()
                        }
                        .disabled(!isValidURL(pasteURL) || uploadState.isLoading)
                    }
                }

                if uploadState.isLoading {
                    Section {
                        HStack(spacing: 10) {
                            ProgressView()
                            Text("Uploading photo…").foregroundStyle(.secondary)
                        }
                    }
                } else if case .failed(let message) = uploadState {
                    Section {
                        Label(message, systemImage: "exclamationmark.triangle")
                            .foregroundStyle(.orange)
                    }
                }
            }
            .navigationTitle("Change photo")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                        .disabled(uploadState.isLoading)
                }
            }
            .onChange(of: photoItem) { _, newItem in
                guard let newItem else { return }
                Task { await handlePicked(newItem) }
            }
        }
    }

    /// Blossom uploads require a real signer + the Blossom capability,
    /// neither of which has a kernel-backed path yet (signer lands at
    /// M1 exit, Blossom at M10). Until then the "Choose from library"
    /// chip surfaces the staged-action banner so the user gets
    /// immediate feedback instead of a silent no-op. The two non-upload
    /// chips ("Choose a style", "Paste image URL") still work in full.
    private func handlePicked(_ item: PhotosPickerItem) async {
        _ = item // accept the picker callback so it doesn't fire repeatedly
        photoItem = nil
        uploadState = .failed(IdentityViewModel.stagedActionToast)
    }

    private func isValidURL(_ s: String) -> Bool {
        guard let url = URL(string: s.trimmed),
              let scheme = url.scheme?.lowercased() else { return false }
        return scheme == "http" || scheme == "https"
    }
}
