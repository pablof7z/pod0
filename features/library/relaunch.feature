Feature: The library is durable Rust-owned state
  Podcast and episode facts live in the Rust store, not in native memory.
  Relaunching the app reopens the same store; only what Rust made durable
  can appear on the other side of that line.

  Scenario: A committed subscription survives an app relaunch
    Given a podcast "Morning Signal" publishes its feed at "https://feeds.example/morning" with episode "Pilot"
    When the app subscribes to the feed at "https://feeds.example/morning"
    And the host completes the feed fetch for "https://feeds.example/morning"
    And the app is relaunched
    Then the library lists the podcast "Morning Signal"
    And the library lists the episode "Pilot"
