Feature: Subscribing to a podcast feed
  Subscribing is the first vertical slice a user feels: the app dispatches
  one typed command, the native host fetches bytes on the core's behalf, and
  Rust owns parsing, storage, and the projection the screen renders. These
  scenarios prove that whole loop across the real facade — the same seven
  operations Swift and Kotlin call.

  Scenario: A subscribed feed's episodes appear in the library
    Given a podcast "Morning Signal" publishes its feed at "https://feeds.example/morning" with episodes "Pilot" and "Second wind"
    When the app subscribes to the feed at "https://feeds.example/morning"
    And the host completes the feed fetch for "https://feeds.example/morning"
    Then the subscription to "https://feeds.example/morning" has succeeded
    And the library lists the podcast "Morning Signal"
    And the library lists the episode "Pilot"
    And the library lists the episode "Second wind"

  Scenario: Malformed feed bytes park the durable fetch with a typed failure
    Given the feed at "https://feeds.example/broken" serves bytes that are not a podcast feed
    When the app subscribes to the feed at "https://feeds.example/broken"
    And the host completes the feed fetch for "https://feeds.example/broken"
    Then the subscription to "https://feeds.example/broken" has succeeded
    And the feed fetch for "https://feeds.example/broken" failed because the feed was malformed
    And no feed fetch work remains for the host
    And the library lists no episodes
