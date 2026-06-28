//! `SocialAction` dispatch — user-identity publishing (kind:0/1/9802) plus
//! kernel-owned peer approve/block allow-list mutations.
//!
//! Extracted from `host_op_handler.rs` to keep that file under the 500-LOC
//! hard ceiling (AGENTS.md). The `handle_social_action` method stays on
//! `PodcastHostOpHandler` via this sibling `impl` block.
//!
//! ## Approve/block actions
//!
//! `ApprovePeer` / `BlockPeer` / `RemoveApproval` / `RemoveBlock` mutate the
//! `ApprovedPeerStore` held in `state.social.approved`, persist it to disk
//! (atomic tmp+rename, D6), then bump `state.social.infra` (Domain::Social)
//! so the `podcast.social` sidecar re-emits with updated `trusted` verdicts
//! on the next tick. This is the real per-domain re-emit site (the #423 lesson:
//! always use `infra.bump()`, never a manual `fetch_add`).

use crate::ffi::actions::social_module::SocialAction;
use crate::host_op_handler::PodcastHostOpHandler;
use crate::store::friends::FriendRecord;
use crate::store::notes::UserNote;

impl PodcastHostOpHandler {
    pub(crate) fn publish_clip_highlight_if_user_visible(
        &self,
        clip: &crate::clip_handler::ClipRecord,
        correlation_id: &str,
    ) {
        crate::social_publish_handler::publish_clip_highlight_if_user_visible(
            self.app,
            &self.state.library.identity,
            &self.state.library.store,
            clip,
            correlation_id,
        );
    }

    pub(crate) fn handle_social_action(
        &self,
        action: SocialAction,
        correlation_id: &str,
    ) -> serde_json::Value {
        match action {
            SocialAction::PublishProfile {
                name,
                display_name,
                about,
                picture,
            } => {
                let result = crate::social_publish_handler::handle_publish_profile(
                    self.app,
                    &self.state.library.identity,
                    &name,
                    display_name.as_deref(),
                    about.as_deref(),
                    picture.as_deref(),
                    correlation_id,
                );
                // Self-apply succeeded: bump the identity domain rev so the push
                // frame re-emits `AccountSummary` with the new display_name /
                // picture_url immediately (established doctrine: bump_domain after
                // mutation, identical to IdentityAction::ImportNsec path).
                if result.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
                    self.bump_domain(crate::state::Domain::Identity);
                }
                result
            }
            SocialAction::PublishNote {
                content,
                episode_coord,
            } => crate::social_publish_handler::handle_publish_note(
                self.app,
                &self.state.library.identity,
                &content,
                episode_coord.as_deref(),
                correlation_id,
            ),
            SocialAction::PublishHighlight {
                content,
                enclosure_url,
                feed_url,
                item_guid,
                start_sec,
                end_sec,
                caption,
            } => {
                let fields = crate::social_publish_handler::HighlightFields {
                    enclosure_url: enclosure_url.as_deref(),
                    feed_url: feed_url.as_deref(),
                    item_guid: item_guid.as_deref(),
                    start_sec,
                    end_sec,
                    caption: caption.as_deref(),
                };
                crate::social_publish_handler::handle_publish_highlight(
                    self.app,
                    &self.state.library.identity,
                    &content,
                    &fields,
                    correlation_id,
                )
            }
            SocialAction::ApprovePeer { pubkey_hex } => {
                self.state.social.approve_peer(&pubkey_hex);
                self.persist_approved_peer_store();
                self.state.social.infra.bump();
                serde_json::json!({"ok": true})
            }
            SocialAction::BlockPeer { pubkey_hex } => {
                self.state.social.block_peer(&pubkey_hex);
                self.persist_approved_peer_store();
                self.state.social.infra.bump();
                serde_json::json!({"ok": true})
            }
            SocialAction::RemoveApproval { pubkey_hex } => {
                self.state.social.remove_peer_approval(&pubkey_hex);
                self.persist_approved_peer_store();
                self.state.social.infra.bump();
                serde_json::json!({"ok": true})
            }
            SocialAction::RemoveBlock { pubkey_hex } => {
                self.state.social.remove_peer_block(&pubkey_hex);
                self.persist_approved_peer_store();
                self.state.social.infra.bump();
                serde_json::json!({"ok": true})
            }
            SocialAction::AddNote {
                id,
                text,
                kind,
                target,
                created_at,
                author,
            } => {
                let changed = self.state.notes.add_note(UserNote {
                    id,
                    text,
                    kind,
                    target,
                    created_at,
                    deleted: false,
                    author,
                });
                serde_json::json!({"ok": true, "changed": changed})
            }
            SocialAction::UpdateNote {
                id,
                text,
                kind,
                target,
            } => {
                let changed = self
                    .state
                    .notes
                    .update_note(&id, text, kind, target.map(Some));
                serde_json::json!({"ok": true, "changed": changed})
            }
            SocialAction::DeleteNote { id } => {
                let changed = self.state.notes.set_deleted(&id, true);
                serde_json::json!({"ok": true, "changed": changed})
            }
            SocialAction::RestoreNote { id } => {
                let changed = self.state.notes.set_deleted(&id, false);
                serde_json::json!({"ok": true, "changed": changed})
            }
            SocialAction::ClearNotes => {
                let changed = self.state.notes.clear_all();
                serde_json::json!({"ok": true, "changed": changed})
            }
            SocialAction::AddFriend {
                id,
                display_name,
                pubkey_hex,
                added_at,
                avatar_url,
                about,
            } => {
                let changed = self.state.friends.add_friend(FriendRecord {
                    id,
                    display_name,
                    pubkey_hex: pubkey_hex.clone(),
                    added_at,
                    avatar_url,
                    about,
                });
                self.state.social.approve_peer(&pubkey_hex);
                self.persist_approved_peer_store();
                self.state.social.infra.bump();
                serde_json::json!({"ok": true, "changed": changed})
            }
            SocialAction::UpdateFriendName { id, display_name } => {
                let (changed, _) = self.state.friends.update_friend_name(&id, display_name);
                serde_json::json!({"ok": true, "changed": changed})
            }
            SocialAction::RemoveFriend { id } => {
                let removed_pubkey = self.state.friends.remove_friend(&id);
                if let Some(pubkey_hex) = removed_pubkey.as_deref() {
                    self.state.social.remove_peer_approval(pubkey_hex);
                    self.persist_approved_peer_store();
                    self.state.social.infra.bump();
                }
                serde_json::json!({"ok": true, "changed": removed_pubkey.is_some()})
            }
        }
    }

    /// Persist the in-memory `ApprovedPeerStore` to disk (atomic tmp+rename).
    ///
    /// D6: failure is logged but not propagated — the in-memory store remains
    /// authoritative for the session.
    fn persist_approved_peer_store(&self) {
        use std::path::PathBuf;
        let data_dir: Option<PathBuf> = match self.state.library.store.lock() {
            Ok(s) => s.data_dir().map(|p| p.to_path_buf()),
            Err(_) => return,
        };
        let Some(dir) = data_dir else { return };
        let Some(ref arc) = self.state.social.approved else { return };
        if let Ok(store) = arc.lock() {
            if let Err(e) =
                crate::store::approved_peer_store::save_approved_peer_store(&dir, &store)
            {
                eprintln!("[social_actions] failed to persist approved-peer-store: {e}");
            }
        }
    }
}
