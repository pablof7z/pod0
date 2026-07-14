import XCTest
@testable import Podcastr

/// Coverage for `SubscriptionService.AddError.errorDescription`.
///
/// These messages are user-facing — they surface inside `AddByURLForm`,
/// `OPMLImportSheet`, and `DiscoverSearchForm` whenever a subscribe call
/// fails. Tests lock the localized copy so a refactor can't silently
/// regress the string a user reads when they paste a typo or hit a
/// network blip.
@MainActor
final class SubscriptionAddErrorTests: XCTestCase {

    func testInvalidURLMessage() {
        let error = SubscriptionService.AddError.invalidURL
        XCTAssertEqual(
            error.errorDescription,
            "That doesn't look like a valid feed URL."
        )
    }

    func testAlreadySubscribedIncludesShowTitle() {
        let error = SubscriptionService.AddError.alreadySubscribed(title: "The Daily")
        XCTAssertEqual(
            error.errorDescription,
            "You're already subscribed to The Daily."
        )
    }

    func testAlreadySubscribedHandlesShowsWithSpecialCharacters() {
        // Title may contain quotes / ampersands / em-dashes — the
        // localized message must pass them through verbatim, not strip.
        let error = SubscriptionService.AddError.alreadySubscribed(title: "AT&T \"Tech\" — News")
        XCTAssertEqual(
            error.errorDescription,
            "You're already subscribed to AT&T \"Tech\" — News."
        )
    }

    func testTransportIncludesUnderlyingMessage() {
        let error = SubscriptionService.AddError.transport("network unreachable")
        XCTAssertEqual(
            error.errorDescription,
            "Couldn't reach the feed: network unreachable"
        )
    }

    func testHTTP404PromptsUserToCheckURL() {
        // 404 is the most common case — paste a wrong URL, expect a
        // hint that points at the URL itself, not the HTTP code.
        let error = SubscriptionService.AddError.http(404)
        XCTAssertEqual(
            error.errorDescription,
            "We couldn't find a feed at that URL. Double-check it and try again."
        )
    }

    func testHTTP410MatchesNotFoundCopy() {
        // 410 Gone is "feed permanently moved or deleted" — same UX
        // intent as 404 (the URL is dead). Lump them together.
        let error = SubscriptionService.AddError.http(410)
        XCTAssertEqual(
            error.errorDescription,
            "We couldn't find a feed at that URL. Double-check it and try again."
        )
    }

    func testHTTP403FlagsAuthRequirement() {
        let error = SubscriptionService.AddError.http(403)
        XCTAssertEqual(
            error.errorDescription,
            "This feed needs sign-in or isn't public — Podcastr can't subscribe to it."
        )
    }

    func testHTTP429SuggestsRetryLater() {
        let error = SubscriptionService.AddError.http(429)
        XCTAssertEqual(
            error.errorDescription,
            "The feed server is rate-limiting requests right now. Try again in a few minutes."
        )
    }

    func testHTTP504TreatedAsTimeout() {
        let error = SubscriptionService.AddError.http(504)
        XCTAssertEqual(
            error.errorDescription,
            "The feed server took too long to respond. Try again in a moment."
        )
    }

    func testHTTP500FlagsServerErrorWithDiagnosticCode() {
        // Server-side 5xx — keep the raw code in parentheses so support
        // can diagnose, but lead with plain English.
        let error = SubscriptionService.AddError.http(500)
        XCTAssertEqual(
            error.errorDescription,
            "The feed server hit an error (HTTP 500). Try again later."
        )
    }

    func testHTTP418FallsThroughToGenericClientError() {
        // 4xx that isn't one of our specific cases — generic copy with
        // the raw code preserved.
        let error = SubscriptionService.AddError.http(418)
        XCTAssertEqual(
            error.errorDescription,
            "The feed server rejected the request (HTTP 418)."
        )
    }

    func testParseSurfacesUnderlyingMessageAsIs() {
        // `.parse` payloads come from `RSSParser.ParseError.errorDescription`,
        // which already speaks in full user-facing sentences. The error
        // surface deliberately doesn't add a "Couldn't read this feed:"
        // prefix to avoid double-narrating the failure.
        let error = SubscriptionService.AddError.parse("malformed XML at line 42")
        XCTAssertEqual(
            error.errorDescription,
            "malformed XML at line 42"
        )
    }

    // MARK: - Equatable contract

    func testInvalidURLEqualsItself() {
        XCTAssertEqual(SubscriptionService.AddError.invalidURL, .invalidURL)
    }

    func testAlreadySubscribedEqualsByTitle() {
        XCTAssertEqual(
            SubscriptionService.AddError.alreadySubscribed(title: "Show"),
            .alreadySubscribed(title: "Show")
        )
        XCTAssertNotEqual(
            SubscriptionService.AddError.alreadySubscribed(title: "Show A"),
            .alreadySubscribed(title: "Show B")
        )
    }

    func testHTTPEqualsByStatusCode() {
        XCTAssertEqual(
            SubscriptionService.AddError.http(404),
            .http(404)
        )
        XCTAssertNotEqual(
            SubscriptionService.AddError.http(404),
            .http(500)
        )
    }

    func testTransportAndParseAreDistinct() {
        // Both wrap a String message but they're separate enum cases —
        // a transport error must never compare equal to a parse error.
        XCTAssertNotEqual(
            SubscriptionService.AddError.transport("oops"),
            .parse("oops")
        )
    }
}
