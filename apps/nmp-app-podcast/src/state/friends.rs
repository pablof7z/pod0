//! Local friends substate.
//!
//! Owns the user-curated friend list in Rust and writes through to
//! `friends.json`. Native shells render `PodcastUpdate.friends` and dispatch
//! mutations here instead of persisting parallel friend arrays.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::state::slot::Session;
use crate::state::{Infra, Slot};
use crate::store::friends::{self, FriendRecord};
use crate::store::PodcastStore;

pub struct FriendsState {
    friends: Slot<Vec<FriendRecord>, Session>,
    infra: Infra,
    store: Arc<Mutex<PodcastStore>>,
}

impl FriendsState {
    pub fn new(infra: Infra, store: Arc<Mutex<PodcastStore>>) -> Self {
        let seed = Self::data_dir_from_store(&store)
            .map(|dir| friends::load_friends(&dir))
            .unwrap_or_default();
        Self {
            friends: Slot::new(seed),
            infra,
            store,
        }
    }

    pub fn friends_snapshot(&self) -> Vec<FriendRecord> {
        self.friends
            .lock()
            .ok()
            .map(|friends| friends.clone())
            .unwrap_or_default()
    }

    pub fn add_friend(&self, friend: FriendRecord) -> bool {
        let changed = if let Ok(mut friends) = self.friends.lock() {
            if let Some(existing) = friends.iter_mut().find(|existing| {
                existing.id == friend.id || existing.pubkey_hex == friend.pubkey_hex
            }) {
                if *existing == friend {
                    false
                } else {
                    *existing = friend;
                    true
                }
            } else {
                friends.push(friend);
                true
            }
        } else {
            false
        };
        self.persist_and_bump(changed);
        changed
    }

    pub fn update_friend_name(&self, id: &str, display_name: String) -> (bool, Option<String>) {
        let mut pubkey = None;
        let changed = if let Ok(mut friends) = self.friends.lock() {
            if let Some(friend) = friends.iter_mut().find(|friend| friend.id == id) {
                pubkey = Some(friend.pubkey_hex.clone());
                if friend.display_name == display_name {
                    false
                } else {
                    friend.display_name = display_name;
                    true
                }
            } else {
                false
            }
        } else {
            false
        };
        self.persist_and_bump(changed);
        (changed, pubkey)
    }

    pub fn remove_friend(&self, id: &str) -> Option<String> {
        let removed = if let Ok(mut friends) = self.friends.lock() {
            let idx = friends.iter().position(|friend| friend.id == id)?;
            Some(friends.remove(idx).pubkey_hex)
        } else {
            None
        };
        self.persist_and_bump(removed.is_some());
        removed
    }

    pub fn set_data_dir(&self, data_dir: &Path) -> bool {
        let restored = friends::load_friends(data_dir);
        let changed = if let Ok(mut friends) = self.friends.lock() {
            if *friends == restored {
                false
            } else {
                *friends = restored;
                true
            }
        } else {
            false
        };
        self.infra.bump_if(changed);
        changed
    }

    fn persist_and_bump(&self, changed: bool) {
        if !changed {
            return;
        }
        if let Some(dir) = Self::data_dir_from_store(&self.store) {
            if let Ok(friends) = self.friends.lock() {
                friends::save_friends(&dir, &friends);
            }
        }
        self.infra.bump();
    }

    fn data_dir_from_store(store: &Arc<Mutex<PodcastStore>>) -> Option<PathBuf> {
        store
            .lock()
            .ok()
            .and_then(|store| store.data_dir().map(PathBuf::from))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_friend(id: &str, name: &str, pubkey: &str) -> FriendRecord {
        FriendRecord {
            id: id.to_string(),
            display_name: name.to_string(),
            pubkey_hex: pubkey.to_string(),
            added_at: 42,
            avatar_url: None,
            about: None,
        }
    }

    #[test]
    fn add_rename_remove_and_project_friend() {
        let store = Arc::new(Mutex::new(PodcastStore::new()));
        let state = FriendsState::new(Infra::for_test(), store);

        assert!(state.add_friend(sample_friend("f1", "Alice", "abc")));
        assert!(!state.add_friend(sample_friend("f1", "Alice", "abc")));
        assert_eq!(state.friends_snapshot().len(), 1);

        assert_eq!(
            state.update_friend_name("f1", "Alice Renamed".to_string()),
            (true, Some("abc".to_string()))
        );
        assert_eq!(state.friends_snapshot()[0].display_name, "Alice Renamed");

        assert_eq!(state.remove_friend("f1"), Some("abc".to_string()));
        assert!(state.friends_snapshot().is_empty());
    }
}
