import Foundation

enum ArtifactRepositoryAuthorityError: Error, Equatable {
    case sharedCoreOwnsTranscripts
}

extension ArtifactRepository {
    func requireNativeWritable(_ records: [ArtifactRecord]) throws {
        if records.contains(where: { $0.kind == .transcript }) {
            throw ArtifactRepositoryAuthorityError.sharedCoreOwnsTranscripts
        }
    }

    func requireNativeWritable(kind: ArtifactKind) throws {
        if kind == .transcript {
            throw ArtifactRepositoryAuthorityError.sharedCoreOwnsTranscripts
        }
    }
}
