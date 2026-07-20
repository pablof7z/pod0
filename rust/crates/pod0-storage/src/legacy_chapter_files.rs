use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use pod0_domain::{ContentDigest, EpisodeId};

use crate::legacy_chapter_episode::ChapterEpisodeContext;
use crate::legacy_chapter_format::{RawAdSpan, RawChapter, RawChapterAttempt};
use crate::legacy_format::uuid_bytes;
use crate::transcript_import_digest::{TranscriptImportHash, digest_bytes, hex_digest};
use crate::{
    ChapterEvidenceKind, ChapterEvidenceValidation, InspectedChapterEvidence, StorageError,
};

const MAX_AUXILIARY_FILE_BYTES: u64 = 16 * 1_024 * 1_024;
const MAX_AUXILIARY_FILES: usize = 100_000;

pub(crate) fn inspect_auxiliary_chapter_files(
    root: &Path,
    referenced: &BTreeSet<PathBuf>,
    episodes: &BTreeMap<EpisodeId, ChapterEpisodeContext>,
) -> Result<Vec<InspectedChapterEvidence>, StorageError> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut entries = Vec::new();
    scan_attempts(root, episodes, &mut entries)?;
    scan_immutable(
        root,
        "chapters",
        ChapterEvidenceKind::UnreferencedChapterFile,
        referenced,
        episodes,
        &mut entries,
    )?;
    scan_immutable(
        root,
        "ads",
        ChapterEvidenceKind::UnreferencedAdFile,
        referenced,
        episodes,
        &mut entries,
    )?;
    Ok(entries)
}

fn scan_attempts(
    root: &Path,
    episodes: &BTreeMap<EpisodeId, ChapterEpisodeContext>,
    entries: &mut Vec<InspectedChapterEvidence>,
) -> Result<(), StorageError> {
    let directory = root.join("attempts").join("chapters");
    for path in json_files_two_levels(&directory)? {
        ensure_capacity(entries)?;
        let (bytes, digest, read_diagnostic) = read_bounded(&path);
        let parsed = serde_json::from_slice::<RawChapterAttempt>(&bytes).ok();
        let episode_id = parsed
            .as_ref()
            .and_then(|attempt| uuid_bytes(&attempt.episode_id, "chapter attempt", 0).ok())
            .map(EpisodeId::from_bytes);
        let path_matches = parsed.as_ref().is_some_and(|attempt| {
            path.parent()
                .and_then(Path::file_name)
                .and_then(|value| value.to_str())
                == Some(attempt.episode_id.as_str())
                && path.file_stem().and_then(|value| value.to_str())
                    == Some(attempt.lease_token.as_str())
                && matches!(
                    attempt.output.chapter_origin.as_str(),
                    "publisher" | "generated" | "publisherEnriched"
                )
                && !attempt.output.chapters.is_empty()
                && attempt.output.chapters.len() <= 4_096
                && attempt.output.ads.len() <= 4_096
        });
        let diagnostic = read_diagnostic
            .or_else(|| (!path_matches).then(|| "attempt_manifest_invalid".to_owned()));
        entries.push(file_entry(
            ChapterEvidenceKind::AttemptManifest,
            &path,
            bytes,
            digest,
            episode_id,
            episodes,
            parsed.as_ref().map(|attempt| attempt.input_version.clone()),
            parsed
                .as_ref()
                .map(|attempt| attempt.output.chapter_origin.clone()),
            diagnostic,
        ));
    }
    Ok(())
}

fn scan_immutable(
    root: &Path,
    directory: &str,
    kind: ChapterEvidenceKind,
    referenced: &BTreeSet<PathBuf>,
    episodes: &BTreeMap<EpisodeId, ChapterEpisodeContext>,
    entries: &mut Vec<InspectedChapterEvidence>,
) -> Result<(), StorageError> {
    for path in json_files_two_levels(&root.join(directory))? {
        let canonical = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if referenced.contains(&canonical) {
            continue;
        }
        ensure_capacity(entries)?;
        let (bytes, digest, read_diagnostic) = read_bounded(&path);
        let episode_id = path
            .parent()
            .and_then(Path::file_name)
            .and_then(|value| value.to_str())
            .and_then(|value| uuid_bytes(value, "chapter file episode", 0).ok())
            .map(EpisodeId::from_bytes);
        let payload_valid = match kind {
            ChapterEvidenceKind::UnreferencedChapterFile => {
                serde_json::from_slice::<Vec<RawChapter>>(&bytes).is_ok()
            }
            ChapterEvidenceKind::UnreferencedAdFile => {
                serde_json::from_slice::<Vec<RawAdSpan>>(&bytes).is_ok()
            }
            _ => false,
        };
        let filename_matches = path
            .file_stem()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value == hex_digest(digest));
        let diagnostic = read_diagnostic.or_else(|| {
            (!payload_valid || !filename_matches || episode_id.is_none())
                .then(|| "unreferenced_artifact_invalid".to_owned())
        });
        entries.push(file_entry(
            kind, &path, bytes, digest, episode_id, episodes, None, None, diagnostic,
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn file_entry(
    kind: ChapterEvidenceKind,
    path: &Path,
    bytes: Vec<u8>,
    digest: ContentDigest,
    episode_id: Option<EpisodeId>,
    episodes: &BTreeMap<EpisodeId, ChapterEpisodeContext>,
    input_version: Option<String>,
    origin: Option<String>,
    diagnostic_code: Option<String>,
) -> InspectedChapterEvidence {
    let path_text = path.to_string_lossy().into_owned();
    let mut hash = TranscriptImportHash::new(b"pod0.legacy-chapter-evidence.v1");
    hash.text(kind.code());
    hash.text(&path_text);
    hash.bytes(&digest.into_bytes());
    let validation = if diagnostic_code.is_some() {
        ChapterEvidenceValidation::Blocked
    } else {
        ChapterEvidenceValidation::Inert
    };
    InspectedChapterEvidence {
        evidence_id: hash.finish(),
        kind,
        source_subject: path_text.clone(),
        episode_id,
        podcast_id: episode_id.and_then(|id| episodes.get(&id).map(|value| value.podcast_id)),
        source_row_id: None,
        legacy_selected: None,
        importer_selected: false,
        source_input_version: input_version,
        source_output_version: None,
        source_origin: origin,
        source_schema_version: Some(1),
        source_integrity: Some("staged".to_owned()),
        source_verified_at_ms: None,
        source_path: Some(path.to_path_buf()),
        source_row_digest: digest,
        raw_digest: digest,
        raw_byte_count: bytes.len() as u64,
        raw_bytes: bytes,
        validation,
        diagnostic_code,
        artifact: None,
        legacy_chapters: Vec::new(),
        legacy_ad_spans: Vec::new(),
    }
}

fn json_files_two_levels(root: &Path) -> Result<Vec<PathBuf>, StorageError> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    for parent in sorted_directory(root)? {
        if !parent
            .file_type()
            .map_err(|error| StorageError::io("inspect chapter evidence directory", error))?
            .is_dir()
        {
            continue;
        }
        for child in sorted_directory(&parent.path())? {
            let file_type = child
                .file_type()
                .map_err(|error| StorageError::io("inspect chapter evidence file", error))?;
            if file_type.is_file()
                && child
                    .path()
                    .extension()
                    .is_some_and(|value| value == "json")
            {
                files.push(child.path());
            }
        }
    }
    Ok(files)
}

fn sorted_directory(path: &Path) -> Result<Vec<fs::DirEntry>, StorageError> {
    let mut entries = fs::read_dir(path)
        .map_err(|error| StorageError::io("read chapter evidence directory", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| StorageError::io("read chapter evidence entry", error))?;
    entries.sort_by_key(fs::DirEntry::file_name);
    Ok(entries)
}

fn read_bounded(path: &Path) -> (Vec<u8>, ContentDigest, Option<String>) {
    let Ok(metadata) = fs::metadata(path) else {
        return (
            Vec::new(),
            digest_bytes(&[]),
            Some("evidence_file_missing".to_owned()),
        );
    };
    if metadata.len() > MAX_AUXILIARY_FILE_BYTES {
        return (
            Vec::new(),
            digest_bytes(&[]),
            Some("evidence_file_too_large".to_owned()),
        );
    }
    let Ok(bytes) = fs::read(path) else {
        return (
            Vec::new(),
            digest_bytes(&[]),
            Some("evidence_file_unreadable".to_owned()),
        );
    };
    let diagnostic = (bytes.len() as u64 != metadata.len())
        .then(|| "evidence_file_changed_during_read".to_owned());
    let digest = digest_bytes(&bytes);
    (bytes, digest, diagnostic)
}

fn ensure_capacity(entries: &[InspectedChapterEvidence]) -> Result<(), StorageError> {
    if entries.len() >= MAX_AUXILIARY_FILES {
        Err(StorageError::ImportLimitExceeded {
            entity: "chapter evidence files",
        })
    } else {
        Ok(())
    }
}
