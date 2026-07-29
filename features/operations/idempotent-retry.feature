Feature: Command retries are idempotent
  A retry with an identical command ID and payload must be a no-op however
  far state has advanced — the contract's protection against a flaky UI or
  a replayed queue repeating a paid or visible effect.

  Scenario: Repeating the identical command envelope issues no second fetch
    Given a podcast "Morning Signal" publishes its feed at "https://feeds.example/morning" with episode "Pilot"
    When the app subscribes to the feed at "https://feeds.example/morning"
    And the app repeats the identical subscribe command for "https://feeds.example/morning"
    Then exactly one feed fetch reaches the host for "https://feeds.example/morning"
