Feature: Cancelling feed hydration stops work and blocks late evidence
  Subscribing commits before feed bytes arrive. Cancelling its still-running
  hydration must clear busy state, withdraw native work that has not started,
  and make a late observation unable to commit anything.

  Scenario: Cancelling before the host accepts withdraws the work
    Given a podcast "Morning Signal" publishes its feed at "https://feeds.example/morning" with episode "Pilot"
    When the app subscribes to the feed at "https://feeds.example/morning"
    And the app cancels the subscription to "https://feeds.example/morning"
    Then no feed fetch work remains for the host
    And no feed fetch workflow remains for "https://feeds.example/morning"
    And the subscription to "https://feeds.example/morning" has succeeded

  Scenario: A late host observation cannot commit after cancellation
    Given a podcast "Morning Signal" publishes its feed at "https://feeds.example/morning" with episode "Pilot"
    When the app subscribes to the feed at "https://feeds.example/morning"
    And the host accepts the pending feed fetch for "https://feeds.example/morning"
    And the app cancels the subscription to "https://feeds.example/morning"
    And the host reports the fetched bytes for "https://feeds.example/morning" anyway
    Then the late feed bytes were refused
    And the state revision has not advanced since the cancellation
    And no feed fetch workflow remains for "https://feeds.example/morning"
    And the subscription to "https://feeds.example/morning" has succeeded
    And the library lists no episodes
