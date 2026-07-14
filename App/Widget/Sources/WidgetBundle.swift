import SwiftUI
import WidgetKit

/// Registers all widgets shipped with the app. Currently a single Now
/// Playing widget — additional widgets (queue, recent episodes, …) drop
/// into the `body` builder.
@main
struct PodcastrWidgetBundle: WidgetBundle {
    var body: some Widget {
        NowPlayingWidget()
    }
}
