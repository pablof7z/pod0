import XCTest
@testable import Podcastr

final class HomeActiveFilterChipsTests: XCTestCase {

    func testNoChipsWhenAllFiltersInactive() {
        let chips = HomeActiveFilters.chips(
            filter: .all,
            categoryID: nil,
            categoryName: { _ in nil }
        )
        XCTAssertTrue(chips.isEmpty)
    }

    func testLibraryFilterProducesChip() {
        let chips = HomeActiveFilters.chips(
            filter: .unplayed,
            categoryID: nil,
            categoryName: { _ in nil }
        )
        XCTAssertEqual(chips.count, 1)
        XCTAssertEqual(chips.first?.label, "Unplayed")
        if case .libraryFilter(let f) = chips.first?.kind {
            XCTAssertEqual(f, .unplayed)
        } else {
            XCTFail("expected libraryFilter chip")
        }
    }

    func testCategoryProducesChipUsingResolvedName() {
        let categoryID = UUID()
        let chips = HomeActiveFilters.chips(
            filter: .all,
            categoryID: categoryID,
            categoryName: { _ in "Tech" }
        )
        XCTAssertEqual(chips.count, 1)
        XCTAssertEqual(chips.first?.label, "Tech")
    }

    func testCategoryWithoutResolvedNameYieldsNoChip() {
        let categoryID = UUID()
        let chips = HomeActiveFilters.chips(
            filter: .all,
            categoryID: categoryID,
            categoryName: { _ in nil }
        )
        XCTAssertTrue(chips.isEmpty)
    }

    func testBothFiltersActiveProducesTwoChipsInOrder() {
        let categoryID = UUID()
        let chips = HomeActiveFilters.chips(
            filter: .downloaded,
            categoryID: categoryID,
            categoryName: { _ in "Tech" }
        )
        XCTAssertEqual(chips.count, 2)
        // Library filter chip leads — its rendering ordering is what the
        // Home strip shows.
        XCTAssertEqual(chips[0].label, "Downloaded")
        XCTAssertEqual(chips[1].label, "Tech")
    }
}
