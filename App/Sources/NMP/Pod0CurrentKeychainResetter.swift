import Foundation

protocol Pod0KeychainResetting {
    func clearAllCurrentSecrets() throws
}

struct Pod0CurrentSecretDeletion {
    let label: String
    let delete: () throws -> Void
}

/// Deletes only Keychain items written by the current Pod0 app. This is used
/// solely for an explicitly confirmed mutually-untrusted-user handoff. It does
/// not scan, detect, import, or adapt any historical namespace.
struct Pod0CurrentKeychainResetter: Pod0KeychainResetting {
    private let deletions: [Pod0CurrentSecretDeletion]

    init(bundleIdentifier: String = Bundle.main.bundleIdentifier ?? "Podcastr") {
        deletions = [
            Pod0CurrentSecretDeletion(label: "human NMP identity") {
                try KeychainStore.deleteString(
                    service: Pod0HumanIdentityLifecycle.localKeychainService(
                        bundleIdentifier: bundleIdentifier
                    ),
                    account: Pod0HumanIdentityLifecycle.localSecretReference
                )
            },
            Pod0CurrentSecretDeletion(label: "human identity catalog") {
                try KeychainPod0IdentityCatalogStorage(
                    bundleIdentifier: bundleIdentifier
                ).clear()
            },
            Pod0CurrentSecretDeletion(label: "agent Nostr identity") {
                try NostrCredentialStore.deletePrivateKey()
            },
            Pod0CurrentSecretDeletion(label: "OpenRouter credential") {
                try OpenRouterCredentialStore.deleteAPIKey()
            },
            Pod0CurrentSecretDeletion(label: "Ollama credential") {
                try OllamaCredentialStore.deleteAPIKey()
            },
            Pod0CurrentSecretDeletion(label: "ElevenLabs credential") {
                try ElevenLabsCredentialStore.deleteAPIKey()
            },
            Pod0CurrentSecretDeletion(label: "AssemblyAI credential") {
                try AssemblyAICredentialStore.deleteAPIKey()
            },
            Pod0CurrentSecretDeletion(label: "Perplexity credential") {
                try PerplexityCredentialStore.deleteAPIKey()
            },
        ]
    }

    init(deletions: [Pod0CurrentSecretDeletion]) {
        self.deletions = deletions
    }

    func clearAllCurrentSecrets() throws {
        var failures: [String] = []
        for deletion in deletions {
            do {
                try deletion.delete()
            } catch {
                failures.append(deletion.label)
            }
        }
        if !failures.isEmpty {
            throw Pod0ResetPolicyError.keychainDeletionFailed(failures)
        }
    }
}
