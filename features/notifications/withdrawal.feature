Feature: New-episode announcements are withdrawn when the user turns them off
  Delivering a system notification is native work the core hands to the
  host. When the user turns notifications off while the host is still
  holding an undelivered announcement, the core must tell the host to
  abandon that exact piece of work.

  Scenario: Turning notifications off tells the host to abandon an accepted announcement
    Given a podcast "Morning Signal" publishes its feed at "https://feeds.example/morning" with episode "Pilot"
    When the app subscribes to the feed at "https://feeds.example/morning"
    And the host completes the feed fetch for "https://feeds.example/morning"
    And the app turns on new-episode notifications for "Morning Signal"
    And the podcast "Morning Signal" adds the episode "Breaking wave" to its feed
    And the app refreshes the podcast "Morning Signal"
    And the host completes the feed fetch for "https://feeds.example/morning"
    And the host accepts the pending announcement of "Breaking wave"
    And the app turns off new-episode notifications
    Then the host is told to abandon the accepted announcement
    And the library lists the episode "Breaking wave"
