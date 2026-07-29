Feature: Event-driven projection delivery
  Native screens subscribe for projections and never poll. Delivery is
  event-driven from the moment of subscription, and unsubscribe ends it
  deterministically — a closed screen must not keep receiving state.

  Scenario: A live library view receives the commit and stops at unsubscribe
    Given a podcast "Morning Signal" publishes its feed at "https://feeds.example/morning" with episode "Pilot"
    And a podcast "Night Owl" publishes its feed at "https://feeds.example/owl" with episode "Insomnia"
    When the app opens a live library view
    And the app subscribes to the feed at "https://feeds.example/morning"
    And the host completes the feed fetch for "https://feeds.example/morning"
    Then the live library view received the podcast "Morning Signal"
    When the app closes the live library view
    And the app subscribes to the feed at "https://feeds.example/owl"
    And the host completes the feed fetch for "https://feeds.example/owl"
    Then no further library deliveries arrived
    And the library lists the podcast "Night Owl"
