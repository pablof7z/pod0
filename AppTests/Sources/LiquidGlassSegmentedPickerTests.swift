import SwiftUI
import XCTest
@testable import Podcastr

/// Pure-logic coverage for `LiquidGlassSegmentedPicker`.
///
/// The view itself isn't snapshot-tested — visual treatment lives in the
/// `#Preview` blocks and the simulator runs. What we *can* verify here is
/// the data plumbing: selection equality, accessibility-label derivation,
/// and the convenience-init's lookup behaviour. Those failures are silent
/// in a UI test and loud in a unit test, so unit-testing them earns its
/// keep.
final class LiquidGlassSegmentedPickerTests: XCTestCase {

    // MARK: - Test fixture

    enum Fixture: String, CaseIterable, Hashable {
        case search = "Search"
        case url = "From URL"
        case opml = "OPML"
    }

    // MARK: - Selection equality

    func testSelectionBindingDrivesValueRoundTrip() {
        var selected: Fixture = .search
        let binding = Binding<Fixture>(
            get: { selected },
            set: { selected = $0 }
        )

        let picker = LiquidGlassSegmentedPicker(
            "Add show source",
            selection: binding,
            segments: Fixture.allCases.map { ($0, $0.rawValue) }
        )

        // Default selection round-trips.
        XCTAssertEqual(picker.values, Fixture.allCases)

        // Mutating the binding flips the underlying state.
        binding.wrappedValue = .url
        XCTAssertEqual(selected, .url)
    }

    // MARK: - Convenience init derives both labels

    func testConvenienceInitDerivesAccessibilityLabelFromSegments() {
        let binding = Binding<Fixture>.constant(.search)
        let picker = LiquidGlassSegmentedPicker(
            "Add show source",
            selection: binding,
            segments: [
                (.search, "Search"),
                (.url, "From URL"),
                (.opml, "OPML"),
            ]
        )

        XCTAssertEqual(picker.accessibilityLabel(.search), "Search")
        XCTAssertEqual(picker.accessibilityLabel(.url), "From URL")
        XCTAssertEqual(picker.accessibilityLabel(.opml), "OPML")
    }

    func testConvenienceInitFallsBackToEmptyStringForUnknownValue() {
        // If a caller passes a subset of segments but the bound selection
        // can hold a value outside that set, the lookup should degrade to
        // an empty string rather than crashing — accessibility stays
        // silent rather than wrong.
        let binding = Binding<Fixture>.constant(.opml)
        let picker = LiquidGlassSegmentedPicker(
            "Add show source",
            selection: binding,
            segments: [
                (.search, "Search"),
                (.url, "From URL"),
            ]
        )

        XCTAssertEqual(picker.accessibilityLabel(.opml), "")
        XCTAssertFalse(picker.values.contains(.opml))
    }

    // MARK: - ViewBuilder init preserves caller's accessibilityLabel

    func testViewBuilderInitUsesProvidedAccessibilityClosure() {
        let binding = Binding<Fixture>.constant(.search)

        let picker = LiquidGlassSegmentedPicker<Fixture, Text>(
            "Mode",
            selection: binding,
            values: Fixture.allCases,
            accessibilityLabel: { "segment-\($0.rawValue)" },
            label: { value, _ in Text(value.rawValue) }
        )

        // ViewBuilder init must route through the supplied closure, not
        // any internal lookup table.
        XCTAssertEqual(picker.accessibilityLabel(.search), "segment-Search")
        XCTAssertEqual(picker.accessibilityLabel(.url), "segment-From URL")
    }

    // MARK: - Values order is preserved

    func testValuesPreserveCallerOrder() {
        let binding = Binding<Fixture>.constant(.search)
        let reordered: [Fixture] = [.opml, .search, .url]

        let picker = LiquidGlassSegmentedPicker<Fixture, Text>(
            "Mode",
            selection: binding,
            values: reordered,
            accessibilityLabel: { $0.rawValue },
            label: { value, _ in Text(value.rawValue) }
        )

        XCTAssertEqual(picker.values, reordered)
    }
}
