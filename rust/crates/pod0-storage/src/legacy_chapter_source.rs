use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use pod0_domain::{ChapterArtifactId, ContentDigest, EpisodeId};

use crate::legacy_chapter_artifact_source::{load_workflow_artifact, workflow_evidence};
use crate::legacy_chapter_db::{LegacyChapterArtifactRow, inspect_legacy_chapter_database};
use crate::legacy_chapter_episode::{ChapterEpisodeContext, inspect_episode_adjunct};
use crate::legacy_chapter_files::inspect_auxiliary_chapter_files;
use crate::transcript_import_digest::TranscriptImportHash;
use crate::{
    ChapterEvidenceKind, ChapterEvidenceValidation, ChapterImportPlan, InspectedChapterEvidence,
    InspectedChapterSource, StorageError,
};

const MAX_EVIDENCE: usize = 200_000;
const MAX_EVIDENCE_BYTES: u64 = 4 * 1_024 * 1_024 * 1_024;

pub fn inspect_legacy_chapter_source(
    database_path: &Path,
    artifact_root: &Path,
) -> Result<ChapterImportPlan, StorageError> {
    inspect_chapter_source(database_path, artifact_root).map(|source| source.plan)
}

pub(crate) fn inspect_chapter_source(
    database_path: &Path,
    artifact_root: &Path,
) -> Result<InspectedChapterSource, StorageError> {
    let database = inspect_legacy_chapter_database(database_path)?;
    let mut entries = Vec::new();
    let mut episodes = BTreeMap::new();
    for (offset, row) in database.episodes.iter().enumerate() {
        let index = u32::try_from(offset).unwrap_or(u32::MAX);
        let (context, entry) =
            inspect_episode_adjunct(row, database.source_kind, database.source_generation, index);
        if let Some(context) = context {
            episodes.insert(context.episode_id, context);
        }
        if let Some(entry) = entry {
            entries.push(entry);
        }
    }
    let loaded = database
        .artifacts
        .iter()
        .map(load_workflow_artifact)
        .collect::<Vec<_>>();
    let pairings = pair_ad_rows(&database.artifacts);
    let mut workflow_entries = Vec::with_capacity(database.artifacts.len());
    for (offset, row) in database.artifacts.iter().enumerate() {
        let context = episode_context(row, &episodes);
        let paired = pairings.get(&offset).map(|index| &loaded[*index]);
        workflow_entries.push(workflow_evidence(
            row,
            &loaded[offset],
            paired,
            context,
            database.source_kind,
            u32::try_from(offset).unwrap_or(u32::MAX),
        ));
    }
    link_paired_ad_identities(&pairings, &mut workflow_entries);
    let selected_workflow_episodes = workflow_entries
        .iter()
        .filter(|entry| {
            entry.kind == ChapterEvidenceKind::WorkflowChapters && entry.importer_selected
        })
        .filter_map(|entry| entry.episode_id)
        .collect::<BTreeSet<_>>();
    for entry in &mut entries {
        if entry.kind == ChapterEvidenceKind::EpisodeAdjunct {
            entry.importer_selected = entry
                .episode_id
                .is_some_and(|episode| !selected_workflow_episodes.contains(&episode));
        }
    }
    entries.extend(workflow_entries);
    let referenced = referenced_files(&database.artifacts);
    entries.extend(inspect_auxiliary_chapter_files(
        artifact_root,
        &referenced,
        &episodes,
    )?);
    entries.sort_by_key(|entry| entry.evidence_id);
    validate_limits(&entries)?;
    let source_selection_digest = selection_digest(database.source_database_digest, &entries);
    let canonical = entries
        .iter()
        .filter_map(|entry| entry.artifact.as_ref().map(|artifact| artifact.artifact_id))
        .collect::<BTreeSet<ChapterArtifactId>>();
    let selected_count = entries
        .iter()
        .filter(|entry| {
            entry.importer_selected
                && matches!(
                    entry.kind,
                    ChapterEvidenceKind::EpisodeAdjunct | ChapterEvidenceKind::WorkflowChapters
                )
        })
        .count();
    let blocked_count = entries
        .iter()
        .filter(|entry| entry.validation == ChapterEvidenceValidation::Blocked)
        .count();
    let plan = ChapterImportPlan {
        source_kind: database.source_kind,
        source_generation: database.source_generation,
        source_file_identity: database.source_file_identity,
        source_database_byte_count: database.source_database_byte_count,
        source_database_digest: database.source_database_digest,
        source_selection_digest,
        evidence_count: checked_count(entries.len(), "chapter evidence")?,
        canonical_artifact_count: checked_count(canonical.len(), "chapter artifacts")?,
        selected_count: checked_count(selected_count, "selected chapter artifacts")?,
        blocked_count: checked_count(blocked_count, "blocked chapter evidence")?,
    };
    Ok(InspectedChapterSource { plan, entries })
}

fn pair_ad_rows(rows: &[LegacyChapterArtifactRow]) -> BTreeMap<usize, usize> {
    let mut pairings = BTreeMap::new();
    for (chapter_index, chapter) in rows.iter().enumerate() {
        if chapter.kind != "chapters" {
            continue;
        }
        let candidates = rows
            .iter()
            .enumerate()
            .filter(|(_, ad)| {
                ad.kind == "adSegments"
                    && ad.subject == chapter.subject
                    && ad.input_version == chapter.input_version
                    && ad.importer_selected == chapter.importer_selected
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        if candidates.len() == 1 {
            pairings.insert(chapter_index, candidates[0]);
        }
    }
    pairings
}

fn link_paired_ad_identities(
    pairings: &BTreeMap<usize, usize>,
    entries: &mut [InspectedChapterEvidence],
) {
    for (chapter, ad) in pairings {
        let identities = entries[*chapter].legacy_ad_spans.clone();
        if entries[*chapter].validation == ChapterEvidenceValidation::Canonical
            && entries[*ad].validation != ChapterEvidenceValidation::Blocked
        {
            entries[*ad].legacy_ad_spans = identities;
        }
    }
}

fn episode_context<'a>(
    row: &LegacyChapterArtifactRow,
    episodes: &'a BTreeMap<EpisodeId, ChapterEpisodeContext>,
) -> Option<&'a ChapterEpisodeContext> {
    let bytes = crate::legacy_format::uuid_bytes(&row.subject, "chapter artifact", 0).ok()?;
    episodes.get(&EpisodeId::from_bytes(bytes))
}

fn referenced_files(rows: &[LegacyChapterArtifactRow]) -> BTreeSet<PathBuf> {
    rows.iter()
        .filter_map(|row| row.location.as_deref())
        .map(|path| fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()))
        .collect()
}

fn selection_digest(
    database_digest: ContentDigest,
    entries: &[InspectedChapterEvidence],
) -> ContentDigest {
    let mut hash = TranscriptImportHash::new(b"pod0.legacy-chapter-selection.v1");
    hash.bytes(&database_digest.into_bytes());
    hash.u64(entries.len() as u64);
    for entry in entries {
        hash.bytes(&entry.evidence_id.into_bytes());
        hash.bytes(&entry.raw_digest.into_bytes());
        hash.u64(entry.raw_byte_count);
        hash.u32(u32::from(entry.importer_selected));
        hash.text(entry.validation.code());
        match &entry.artifact {
            Some(artifact) => {
                hash.u32(1);
                hash.bytes(&artifact.artifact_id.into_bytes());
                hash.bytes(&artifact.integrity_digest.into_bytes());
            }
            None => hash.u32(0),
        }
    }
    hash.finish()
}

fn validate_limits(entries: &[InspectedChapterEvidence]) -> Result<(), StorageError> {
    if entries.len() > MAX_EVIDENCE {
        return Err(StorageError::ImportLimitExceeded {
            entity: "chapter evidence",
        });
    }
    let mut bytes = 0_u64;
    for entry in entries {
        bytes =
            bytes
                .checked_add(entry.raw_byte_count)
                .ok_or(StorageError::ImportLimitExceeded {
                    entity: "chapter evidence bytes",
                })?;
        if bytes > MAX_EVIDENCE_BYTES {
            return Err(StorageError::ImportLimitExceeded {
                entity: "chapter evidence bytes",
            });
        }
    }
    Ok(())
}

fn checked_count(value: usize, entity: &'static str) -> Result<u32, StorageError> {
    u32::try_from(value).map_err(|_| StorageError::ImportLimitExceeded { entity })
}
