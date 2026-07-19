use serde::Deserialize;

use crate::legacy_format::RawTimestamp;

#[derive(Deserialize)]
pub(crate) struct RawNote {
    pub(crate) id: String,
    #[serde(default)]
    pub(crate) text: String,
    pub(crate) kind: Option<String>,
    pub(crate) target: Option<RawNoteTarget>,
    #[serde(rename = "createdAt")]
    pub(crate) created_at: Option<RawTimestamp>,
    #[serde(default)]
    pub(crate) deleted: bool,
    pub(crate) author: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct RawNoteTarget {
    pub(crate) kind: String,
    pub(crate) id: String,
    #[serde(rename = "positionSeconds")]
    pub(crate) position_seconds: Option<f64>,
}
