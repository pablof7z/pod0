//! `When` — the acts. Each step is one thing the app or the native host
//! does at the facade boundary: dispatch a typed command, accept bounded
//! host work, report one typed observation, open or close a live view,
//! relaunch the process. The first act in a scenario starts the world.

use cucumber::when;

use crate::world::PodWorld;

#[when(regex = r#"^the app subscribes to the feed at "([^"]+)"$"#)]
async fn app_subscribes(w: &mut PodWorld, url: String) {
    w.subscribe_to_feed(&url);
}

#[when(regex = r#"^the app repeats the identical subscribe command for "([^"]+)"$"#)]
async fn app_repeats_subscribe(w: &mut PodWorld, url: String) {
    w.repeat_subscribe(&url);
}

#[when(regex = r#"^the host accepts the pending feed fetch for "([^"]+)"$"#)]
async fn host_accepts_fetch(w: &mut PodWorld, url: String) {
    w.accept_fetch_work(&url);
}

#[when(regex = r#"^the host completes the feed fetch for "([^"]+)"$"#)]
async fn host_completes_fetch(w: &mut PodWorld, url: String) {
    w.complete_fetch_work(&url);
}

/// The cancelled-work variant: the same observation, delivered after the
/// app withdrew the request. Its own step so the scenario says out loud
/// that the host is acting on stale work.
#[when(regex = r#"^the host reports the fetched bytes for "([^"]+)" anyway$"#)]
async fn host_reports_late(w: &mut PodWorld, url: String) {
    w.complete_fetch_work(&url);
}

#[when(regex = r#"^the app cancels the subscription to "([^"]+)"$"#)]
async fn app_cancels(w: &mut PodWorld, url: String) {
    w.cancel_subscribe(&url);
}

#[when(regex = r#"^the podcast "([^"]+)" adds the episode "([^"]+)" to its feed$"#)]
async fn podcast_adds_episode(w: &mut PodWorld, podcast: String, episode: String) {
    w.add_staged_episode(&podcast, &episode);
}

#[when(regex = r#"^the app refreshes the podcast "([^"]+)"$"#)]
async fn app_refreshes(w: &mut PodWorld, podcast: String) {
    w.refresh_podcast(&podcast);
}

#[when(regex = r#"^the app turns on new-episode notifications for "([^"]+)"$"#)]
async fn app_enables_notifications(w: &mut PodWorld, podcast: String) {
    w.enable_notifications_for(&podcast);
}

#[when(regex = r#"^the app turns off new-episode notifications$"#)]
async fn app_disables_notifications(w: &mut PodWorld) {
    w.disable_notifications();
}

#[when(regex = r#"^the host accepts the pending announcement of "([^"]+)"$"#)]
async fn host_accepts_announcement(w: &mut PodWorld, episode: String) {
    w.accept_announcement(&episode);
}

#[when(regex = r#"^the app is relaunched$"#)]
async fn app_relaunches(w: &mut PodWorld) {
    w.relaunch();
}

#[when(regex = r#"^the app opens a live library view$"#)]
async fn app_opens_library_view(w: &mut PodWorld) {
    w.open_library_watch();
}

#[when(regex = r#"^the app closes the live library view$"#)]
async fn app_closes_library_view(w: &mut PodWorld) {
    w.close_library_watch();
}
