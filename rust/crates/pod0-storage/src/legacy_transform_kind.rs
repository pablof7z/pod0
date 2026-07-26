use pod0_domain::PodcastKind;

use crate::legacy_format::unknown_wire_code;

pub(crate) fn podcast_kind(value: &str) -> PodcastKind {
    match value {
        "rss" => PodcastKind::Rss,
        "synthetic" => PodcastKind::Synthetic,
        other => PodcastKind::Unsupported {
            wire_code: unknown_wire_code(other),
        },
    }
}
