use super::handle::PodcastHandle;
use super::projections::AccountSummary;
use crate::store::identity::IdentityStore;

/// Build the active-account identity projection from the app-owned identity
/// store. `npub` is display-friendly; `pubkey_hex` is the canonical account id
/// hosts use for signing, profile lookup, allowlists, and removal.
pub(crate) fn build_active_account(handle: &PodcastHandle) -> Option<AccountSummary> {
    handle
        .identity
        .lock()
        .ok()
        .and_then(|id| project_active_account(&id))
}

fn project_active_account(id: &IdentityStore) -> Option<AccountSummary> {
    let npub = id.npub.as_ref()?;
    let pubkey_hex = id.pubkey_hex.as_ref()?;
    Some(AccountSummary {
        npub: npub.clone(),
        pubkey_hex: pubkey_hex.clone(),
        mode: "local_key".into(),
        display_name: id.display_name.clone(),
        picture_url: id.picture_url.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_NSEC: &str = "nsec1cdxlq0ckkqeuauhzqaduugmrjpwuk3cgwq37ef2nvzje8at26lwqapk9us";
    const TEST_PUBKEY_HEX: &str =
        "c7f5c9fc41894086a2fd8c3e542c1d6e6beeb2175ba41813de38bd02936bd4ff";
    const TEST_NPUB: &str = "npub1cl6unlzp39qgdgha3sl9gtqade47avshtwjpsy778z7s9ymt6nls2thmtl";

    #[test]
    fn active_account_projects_hex_and_npub() {
        let mut identity = IdentityStore::new();
        identity.import_nsec(TEST_NSEC).expect("valid fixture key");
        identity.display_name = Some("Pod0 User".into());

        let account = project_active_account(&identity).expect("active account");

        assert_eq!(account.pubkey_hex, TEST_PUBKEY_HEX);
        assert_eq!(account.npub, TEST_NPUB);
        assert_eq!(account.display_name.as_deref(), Some("Pod0 User"));
        assert_eq!(account.mode, "local_key");
    }

    #[test]
    fn active_account_is_absent_without_canonical_hex() {
        let mut identity = IdentityStore::new();
        identity.npub = Some(TEST_NPUB.into());

        assert!(project_active_account(&identity).is_none());
    }
}
