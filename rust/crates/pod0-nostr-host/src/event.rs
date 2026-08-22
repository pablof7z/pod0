use nostr::{
    ClientMessage, Event, EventBuilder, JsonUtil as _, Kind, PublicKey, Tag, Timestamp,
};
use pod0_application::Pod0PublicationDraft;

use crate::{DraftError, SigningSecret};

pub(crate) struct SignedDraft {
    pub event: Event,
    pub wire_message: String,
}

pub(crate) fn sign_exact_draft(
    draft: &Pod0PublicationDraft,
    signing_secret: &SigningSecret,
) -> Result<SignedDraft, DraftError> {
    if !is_lower_hex(&draft.expected_author_hex, 64) {
        return Err(DraftError::InvalidExpectedAuthor);
    }
    let expected = PublicKey::from_hex(&draft.expected_author_hex)
        .map_err(|_| DraftError::InvalidExpectedAuthor)?;
    if expected != signing_secret.public_key() {
        return Err(DraftError::AuthorMismatch {
            expected_author_hex: draft.expected_author_hex.clone(),
            configured_author_hex: signing_secret.public_key_hex(),
        });
    }
    let tags = draft
        .tags
        .iter()
        .enumerate()
        .map(|(index, tag)| Tag::parse(tag.clone()).map_err(|_| DraftError::InvalidTag { index }))
        .collect::<Result<Vec<_>, _>>()?;
    let event = EventBuilder::new(Kind::from_u16(draft.kind), draft.content.clone())
        .tags(tags)
        .custom_created_at(Timestamp::from_secs(draft.created_at_seconds))
        .allow_self_tagging()
        .build(signing_secret.public_key());
    let event = signing_secret
        .sign_event(event)
        .map_err(|_| DraftError::SigningFailed)?;
    if event.pubkey != expected
        || event.created_at.as_secs() != draft.created_at_seconds
        || event.kind.as_u16() != draft.kind
        || event.content != draft.content
        || event.tags.len() != draft.tags.len()
        || event
            .tags
            .iter()
            .zip(&draft.tags)
            .any(|(actual, exact)| actual.as_slice() != exact)
        || event.verify().is_err()
    {
        return Err(DraftError::EventConstructionChangedDraft);
    }
    let wire_message = ClientMessage::event(event.clone())
        .try_as_json()
        .map_err(|_| DraftError::EventSerializationFailed)?;
    Ok(SignedDraft {
        event,
        wire_message,
    })
}

fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use pod0_application::Pod0PublicationDraft;
    use pod0_domain::PublicationId;

    use super::*;

    const SECRET: &str = "0000000000000000000000000000000000000000000000000000000000000001";
    const AUTHOR: &str = "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";

    #[test]
    fn real_signature_preserves_every_event_field_including_self_tags() {
        let draft = Pod0PublicationDraft {
            publication_id: PublicationId::from_parts(1, 2),
            expected_author_hex: AUTHOR.into(),
            correlation_token: "not-an-event-field".into(),
            created_at_seconds: 1_800_000_000,
            kind: 30_075,
            tags: vec![
                vec!["d".into(), "episode".into()],
                vec!["p".into(), AUTHOR.into()],
                vec!["x-custom".into(), "exact".into(), "".into()],
            ],
            content: "exact publication payload".into(),
        };
        let secret = SigningSecret::parse(SECRET).unwrap();
        let signed = sign_exact_draft(&draft, &secret).unwrap();
        let signed_again = sign_exact_draft(&draft, &secret).unwrap();

        assert!(signed.event.verify().is_ok());
        assert_eq!(signed.event.id, signed_again.event.id);
        assert_eq!(signed.event.pubkey.to_hex(), AUTHOR);
        assert_eq!(signed.event.created_at.as_secs(), draft.created_at_seconds);
        assert_eq!(signed.event.kind.as_u16(), draft.kind);
        assert_eq!(signed.event.content, draft.content);
        assert_eq!(
            signed
                .event
                .tags
                .iter()
                .map(|tag| tag.as_slice().to_vec())
                .collect::<Vec<_>>(),
            draft.tags
        );
        assert!(signed.wire_message.starts_with("[\"EVENT\","));
    }

    #[test]
    fn signer_mismatch_is_refused_before_event_construction() {
        let draft = Pod0PublicationDraft {
            publication_id: PublicationId::from_parts(1, 2),
            expected_author_hex: "11".repeat(32),
            correlation_token: "correlation".into(),
            created_at_seconds: 1,
            kind: 1,
            tags: vec![],
            content: "payload".into(),
        };
        let secret = SigningSecret::parse(SECRET).unwrap();
        assert!(matches!(
            sign_exact_draft(&draft, &secret),
            Err(DraftError::AuthorMismatch { .. })
        ));
    }
}
