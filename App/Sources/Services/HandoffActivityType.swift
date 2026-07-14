import Foundation

/// Activity-type identifiers registered in Info.plist under `NSUserActivityTypes`.
/// Used to donate and receive Handoff continuations so the user can resume
/// editing an item seamlessly on another device.
enum HandoffActivityType {
    /// Editing a single item. `userInfo` keys are defined in ``HandoffUserInfoKey``.
    static let editItem = "com.podcastr.podcastr.editItem"
}

/// `userInfo` keys used in the ``HandoffActivityType/editItem`` activity.
enum HandoffUserInfoKey {
    /// The `UUID.uuidString` of the item being edited.
    static let itemID = "itemID"
    /// The item's current title (snapshot so the receiving device can pre-fill
    /// compose even if the item has not synced yet).
    static let itemTitle = "itemTitle"
}
