//! Recoverable conversion of accepted v1 records into typed v2 sources.
//!
//! The accepted bundle remains an immutable historical representation.  The
//! typed source is added alongside it, and the normal store revision is
//! advanced once so callers have one CAS value for both representations.

use super::*;
use std::{fs, path::{Path, PathBuf}};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MigrationRecord {
    pub workspace: PathBuf,
    pub mission_id: Uuid,
    pub source_revision: u64,
    pub payload_digest: String,
    pub snapshot: PathBuf,
    pub state: MigrationState,
    #[serde(default)]
    pub diagnostic: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum MigrationState { Planned, Applied, Verified, RolledBack, Failed }

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MigrationJournal { pub id: Uuid, pub records: Vec<MigrationRecord> }

impl MissionLifecycle {
    /// Plans all records in explicitly selected workspaces.  Planning reads no
    /// mutable store files and rejects a record rather than using an opaque
    /// Markdown fallback.
    pub fn plan_migration(roots: &[PathBuf]) -> Result<MigrationJournal, MissionError> {
        let mut records = Vec::new();
        for workspace in roots {
            let store = MissionStore::open(workspace);
            if !store.root.is_dir() { continue; }
            let mut directories = fs::read_dir(&store.root)?
                .filter_map(Result::ok).filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
                .collect::<Vec<_>>();
            directories.sort_by_key(|e| e.file_name());
            for entry in directories {
                let Ok(mission_id) = entry.file_name().to_string_lossy().parse() else { continue };
                let accepted = store.get(mission_id)?;
                if MissionLifecycle::open(workspace).get_mission_source(mission_id).is_ok() {
                    continue;
                }
                let converted = convert(&accepted)?;
                records.push(MigrationRecord {
                    workspace: workspace.clone(), mission_id,
                    source_revision: accepted.bundle.manifest.revision,
                    payload_digest: digest(&serde_json::to_vec(&converted)?),
                    snapshot: store.root.join(".migration-snapshots").join(mission_id.to_string()),
                    state: MigrationState::Planned, diagnostic: None,
                });
            }
        }
        Ok(MigrationJournal { id: Uuid::new_v4(), records })
    }

    /// Applies only planned/failed records with their original CAS.  Each
    /// record directory is copied before mutation, making a batch intentionally
    /// non-atomic but individually recoverable.
    pub fn apply_migration(journal: &mut MigrationJournal) -> Result<(), MissionError> {
        persist(journal)?;
        for record in &mut journal.records {
            if !matches!(record.state, MigrationState::Planned | MigrationState::Failed) { continue; }
            let store = MissionStore::open(&record.workspace);
            let current = match store.get(record.mission_id) {
                Ok(value) => value,
                Err(error) => { record.state = MigrationState::Failed; record.diagnostic = Some(error.to_string()); continue; }
            };
            if current.bundle.manifest.revision != record.source_revision {
                record.state = MigrationState::Failed;
                record.diagnostic = Some("source revision changed; original CAS no longer applies".into());
                continue;
            }
            let source = match convert(&current) {
                Ok(source) => source,
                Err(error) => { record.state = MigrationState::Failed; record.diagnostic = Some(error.to_string()); continue; }
            };
            if digest(&serde_json::to_vec(&source)?) != record.payload_digest {
                record.state = MigrationState::Failed; record.diagnostic = Some("planned typed payload changed".into()); continue;
            }
            let directory = store.root.join(record.mission_id.to_string());
            let snapshot = record.snapshot.join(journal.id.to_string());
            if let Err(error) = copy_tree(&directory, &snapshot) {
                record.state = MigrationState::Failed; record.diagnostic = Some(error.to_string()); continue;
            }
            let mut next = current.bundle.clone();
            next.manifest.revision += 1;
            let result = store.import(record.mission_id, next, record.source_revision)
                .and_then(|_| MissionLifecycle::open(&record.workspace).save_migrated(source));
            match result {
                Ok(()) => { record.state = MigrationState::Applied; }
                Err(error) => {
                    let _ = fs::remove_dir_all(&directory);
                    let _ = fs::rename(&snapshot, &directory);
                    record.state = MigrationState::Failed; record.diagnostic = Some(error.to_string());
                }
            }
        }
        persist(journal)?;
        Ok(())
    }

    pub fn read_migration(journal: &MigrationJournal) -> &MigrationJournal { journal }

    pub fn resume_migration(journal: &mut MigrationJournal) -> Result<(), MissionError> {
        Self::apply_migration(journal)
    }

    /// Restores only records this journal applied and refuses drifted records.
    pub fn rollback_migration(journal: &mut MigrationJournal) -> Result<(), MissionError> {
        for record in &mut journal.records {
            if record.state != MigrationState::Applied { continue; }
            let store = MissionStore::open(&record.workspace);
            let current = store.get(record.mission_id)?;
            if current.bundle.manifest.revision != record.source_revision + 1 {
                record.diagnostic = Some("migration target drifted; refusing rollback".into());
                continue;
            }
            let directory = store.root.join(record.mission_id.to_string());
            let snapshot = record.snapshot.join(journal.id.to_string());
            fs::remove_dir_all(&directory)?;
            fs::rename(snapshot, directory)?;
            record.state = MigrationState::RolledBack;
        }
        persist(journal)?;
        Ok(())
    }

    fn save_migrated(&self, source: TypedDraft) -> Result<(), MissionError> {
        // The accepted CAS was advanced first; source.json did not exist in a
        // v1 record, so zero is the source-side CAS precondition.
        self.save_typed(&source, 0, false)
    }
}

fn convert(accepted: &AcceptedMission) -> Result<TypedDraft, MissionError> {
    Ok(TypedDraft {
        mission_id: accepted.mission_id, schema_version: TYPED_SOURCE_SCHEMA_VERSION,
        revision: accepted.bundle.manifest.revision + 1,
        title: accepted.bundle.manifest.title.clone(), objective: accepted.bundle.manifest.objective.clone(),
        waypoints: accepted.bundle.waypoints.iter().map(|w| Ok(TypedWaypoint {
            waypoint_id: w.waypoint_id.clone(), title: w.title.clone(), prompt: w.prompt.clone(),
            part: PartSource { waypoint_id: w.waypoint_id.clone(), document: parse_markdown(&w.part_markdown)? },
            depends_on: w.depends_on.clone(), validation: w.validation.clone(),
            commit_checkpoint: w.commit_checkpoint.clone(), non_goals: w.non_goals.clone(),
            ownership_refs: OwnershipRefs {
                ticket_id: w.ticket_ref.as_deref().and_then(|s| s.parse().ok()),
                child_mission_id: w.child_mission_ref.as_deref().and_then(|s| s.parse().ok()),
            },
        })).collect::<Result<Vec<_>, MissionError>>()?,
    })
}

// This deliberately accepts only the W2 block vocabulary.  A non-empty line
// is always represented as text, so no source bytes are silently discarded.
fn parse_markdown(input: &str) -> Result<Document, MissionError> {
    let mut blocks = Vec::new();
    let mut lines = input.lines().peekable();
    while let Some(line) = lines.next() {
        if line.trim().is_empty() { continue; }
        if line.starts_with("```") {
            let language = line.trim_start_matches("```").to_string();
            let mut text = String::new(); let mut closed = false;
            for value in lines.by_ref() { if value.starts_with("```") { closed = true; break; } text.push_str(value); text.push('\n'); }
            if !closed { return Err(migration_error("unclosed fenced code block")); }
            blocks.push(Block::CodeFence { language, text: text.trim_end_matches('\n').into() }); continue;
        }
        if line == "---" || line == "***" { blocks.push(Block::ThematicBreak); continue; }
        if let Some((hashes, text)) = line.split_once(' ') {
            if !hashes.is_empty() && hashes.bytes().all(|b| b == b'#') && hashes.len() <= 6 {
                blocks.push(Block::Heading { level: hashes.len() as u8, inlines: parse_inlines(text)? }); continue;
            }
        }
        if line.starts_with('|') && line.ends_with('|') {
            let cells = |s: &str| s.trim_matches('|').split('|').map(|v| parse_inlines(v.trim())).collect::<Result<Vec<_>, _>>();
            let header = cells(line)?;
            if lines.peek().is_some_and(|v| v.contains("---")) { lines.next(); }
            let mut rows = Vec::new();
            while lines.peek().is_some_and(|v| v.starts_with('|') && v.ends_with('|')) { rows.push(cells(lines.next().unwrap())?); }
            blocks.push(Block::Table { header, rows }); continue;
        }
        if let Some(text) = line.strip_prefix("> ") { blocks.push(Block::Blockquote { blocks: vec![Block::Paragraph { inlines: parse_inlines(text)? }] }); continue; }
        if let Some(text) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) {
            blocks.push(Block::BulletList { items: vec![vec![Block::Paragraph { inlines: parse_inlines(text)? }]] }); continue;
        }
        if let Some((number, text)) = line.split_once(". ") && number.bytes().all(|b| b.is_ascii_digit()) {
            blocks.push(Block::OrderedList { items: vec![vec![Block::Paragraph { inlines: parse_inlines(text)? }]] }); continue;
        }
        blocks.push(Block::Paragraph { inlines: parse_inlines(line)? });
    }
    Ok(Document { blocks })
}

fn parse_inlines(input: &str) -> Result<Vec<Inline>, MissionError> {
    // Preserve all bytes while recognizing the inline syntax encountered in
    // Mission Parts.  Invalid delimiters remain text, rather than disappearing.
    let mut out = Vec::new(); let mut rest = input;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("**") && let Some(end) = after.find("**") {
            out.push(Inline::Strong { inlines: parse_inlines(&after[..end])? }); rest = &after[end + 2..]; continue;
        }
        if let Some(after) = rest.strip_prefix('*') && let Some(end) = after.find('*') {
            out.push(Inline::Emphasis { inlines: parse_inlines(&after[..end])? }); rest = &after[end + 1..]; continue;
        }
        if let Some(after) = rest.strip_prefix('`') && let Some(end) = after.find('`') {
            out.push(Inline::Code { text: after[..end].into() }); rest = &after[end + 1..]; continue;
        }
        if let Some(after) = rest.strip_prefix('[') && let Some(mid) = after.find("](") && let Some(end) = after[mid + 2..].find(')') {
            out.push(Inline::Link { inlines: parse_inlines(&after[..mid])?, href: after[mid + 2..mid + 2 + end].into() }); rest = &after[mid + 3 + end..]; continue;
        }
        let index = rest.char_indices().nth(1).map_or(rest.len(), |(i, _)| i);
        out.push(Inline::Text { text: rest[..index].into() }); rest = &rest[index..];
    }
    Ok(out)
}

fn migration_error(message: &str) -> MissionError { MissionError::Validation(1, vec![Diagnostic { code: "migration-parse", source: "part_markdown".into(), pointer: String::new(), waypoint_id: None, message: message.into() }]) }
fn digest(bytes: &[u8]) -> String { let mut h = 0xcbf29ce484222325u64; for b in bytes { h = (h ^ u64::from(*b)).wrapping_mul(0x100000001b3); } format!("{h:016x}") }
fn copy_tree(source: &Path, destination: &Path) -> std::io::Result<()> {
    if destination.exists() { fs::remove_dir_all(destination)?; }
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? { let entry = entry?; let target = destination.join(entry.file_name()); if entry.file_type()?.is_dir() { copy_tree(&entry.path(), &target)?; } else { fs::copy(entry.path(), target)?; } }
    Ok(())
}
fn persist(journal: &MigrationJournal) -> Result<(), MissionError> {
    let bytes = serde_json::to_vec_pretty(journal)?;
    let mut roots = BTreeSet::new();
    for record in &journal.records { roots.insert(record.workspace.clone()); }
    for root in roots {
        let path = MissionStore::open(&root).root.join(".migration-journal");
        fs::create_dir_all(&path)?;
        fs::write(path.join(format!("{}.json", journal.id)), &bytes)?;
    }
    Ok(())
}
