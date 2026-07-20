/// The tabs available at the root navigation level.
///
/// Search is reachable via the toolbar. The Player lives behind a persistent
/// mini-bar, while Settings and Saved are reachable from the avatar sidebar.
enum RootTab: String, CaseIterable {
    case home = "Home"
    case library = "Library"
    case saved = "Saved"

    var icon: String {
        switch self {
        case .home: "house.fill"
        case .library: "tray.fill"
        case .saved: "bookmark.fill"
        }
    }
}
