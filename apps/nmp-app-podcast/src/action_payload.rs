//! Podcast-specific action payloads (FlatBuffers ADR-0064 contract).
//!
//! This module implements the `ActionPayload` trait for podcast namespace
//! actions, enabling typed dispatch through `nmp_app_dispatch_action_bytes`.
//! The payload schema is a simple FlatBuffers table carrying JSON that the
//! registry decodes before feeding to action modules.

use nmp_core::substrate::{ActionPayload, ActionPayloadDecodeError};

/// A generic FlatBuffers-backed payload for podcast actions.
/// Carries a schema version and opaque JSON body.
#[derive(Clone, Debug)]
pub struct PodcastJsonPayload {
    pub schema_version: u32,
    pub body_json: String,
}

impl ActionPayload for PodcastJsonPayload {
    const SCHEMA_ID: &'static str = "podcast.json_passthrough";
    const SCHEMA_VERSION: u32 = 1;

    fn decode(bytes: &[u8]) -> Result<Self, ActionPayloadDecodeError> {
        // For now, use a simple passthrough that wraps raw JSON.
        // In the future, this could use generated FlatBuffers readers.

        // Verify minimal FlatBuffers structure
        if bytes.is_empty() {
            return Err(ActionPayloadDecodeError::Malformed {
                reason: "empty payload".to_string(),
            });
        }

        // Extract schema version (byte 0) and JSON body (remaining bytes)
        // This is a simplified implementation; a real FlatBuffers decode
        // would use generated reader code.
        let schema_version = if bytes.len() >= 4 {
            u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
        } else {
            1u32
        };

        if schema_version != Self::SCHEMA_VERSION {
            return Err(ActionPayloadDecodeError::SchemaVersionMismatch {
                found: schema_version,
                expected: Self::SCHEMA_VERSION,
            });
        }

        // The JSON body follows the version bytes
        let body_start = if bytes.len() > 4 { 4 } else { 0 };
        let body_json = String::from_utf8(bytes[body_start..].to_vec()).map_err(|e| {
            ActionPayloadDecodeError::Malformed {
                reason: format!("invalid UTF-8 in payload body: {}", e),
            }
        })?;

        Ok(PodcastJsonPayload {
            schema_version,
            body_json,
        })
    }

    fn encode(&self) -> Vec<u8> {
        // Encode schema version as 4 bytes, followed by JSON
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&self.schema_version.to_le_bytes());
        bytes.extend_from_slice(self.body_json.as_bytes());
        bytes
    }
}

/// Decode a `PodcastJsonPayload` buffer into a concrete action type `A`.
///
/// This is the canonical `ActionModule::decode_payload` body for all
/// podcast-namespace modules: decode the typed buffer as a
/// [`PodcastJsonPayload`], then JSON-deserialize `body_json` into `A`.
/// Always returns `Some(_)` — podcast modules are typed-payload-capable
/// (ADR-0064 / S3 #1751).
pub fn decode_podcast_payload<A: serde::de::DeserializeOwned>(
    bytes: &[u8],
) -> Option<Result<A, ActionPayloadDecodeError>> {
    Some(
        <PodcastJsonPayload as ActionPayload>::decode(bytes).and_then(|p| {
            serde_json::from_str::<A>(&p.body_json).map_err(|e| {
                ActionPayloadDecodeError::Malformed {
                    reason: format!("failed to deserialize action: {e}"),
                }
            })
        }),
    )
}

/// Macro to implement `ActionPayload` for a podcast action type.
/// Deserializes `body_json` from `PodcastJsonPayload` into the target type.
#[macro_export]
macro_rules! impl_podcast_action_payload {
    ($action_ty:ty, $schema_id:literal, $version:expr) => {
        impl $crate::nmp_core::substrate::ActionPayload for $action_ty {
            const SCHEMA_ID: &'static str = $schema_id;
            const SCHEMA_VERSION: u32 = $version;

            fn decode(
                bytes: &[u8],
            ) -> Result<Self, $crate::nmp_core::substrate::ActionPayloadDecodeError> {
                let payload = $crate::action_payload::PodcastJsonPayload::decode(bytes)?;
                serde_json::from_str::<Self>(&payload.body_json).map_err(|e| {
                    $crate::nmp_core::substrate::ActionPayloadDecodeError::Malformed {
                        reason: format!("failed to deserialize action JSON: {}", e),
                    }
                })
            }

            fn encode(&self) -> Vec<u8> {
                let body_json = serde_json::to_string(self).unwrap_or_default();
                let payload = $crate::action_payload::PodcastJsonPayload {
                    schema_version: Self::SCHEMA_VERSION,
                    body_json,
                };
                payload.encode()
            }
        }
    };
}
