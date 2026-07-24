/// The tabs available at the root navigation level.
///
/// The Player lives behind a persistent mini-bar, while Clips and Settings are
/// reachable from the avatar sidebar.
enum RootTab: String, CaseIterable {
    case home = "Home"
    case library = "Library"
    case clips = "Clips"
    case settings = "Settings"

    var icon: String {
        switch self {
        case .home: "house.fill"
        case .library: "tray.fill"
        case .clips: "scissors"
        case .settings: "gearshape"
        }
    }
}
