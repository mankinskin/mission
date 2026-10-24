use super::*;
use schemars::JsonSchema;

pub const TYPED_SOURCE_SCHEMA_VERSION: u32 = 2;
pub const PART_SOURCE_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TypedDraft {
    #[schemars(with = "String")]
    pub mission_id: Uuid,
    pub schema_version: u32,
    pub revision: u64,
    pub title: String,
    pub objective: String,
    #[serde(default)]
    pub waypoints: Vec<TypedWaypoint>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TypedWaypoint {
    pub waypoint_id: String,
    pub title: String,
    pub prompt: String,
    pub part: PartSource,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub validation: Vec<ValidationGate>,
    #[serde(default)]
    pub commit_checkpoint: CommitCheckpoint,
    #[serde(default)]
    pub non_goals: Vec<String>,
    #[serde(default)]
    pub ownership_refs: OwnershipRefs,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OwnershipRefs {
    #[serde(default)]
    #[schemars(with = "Option<String>")]
    pub ticket_id: Option<Uuid>,
    #[serde(default)]
    #[schemars(with = "Option<String>")]
    pub child_mission_id: Option<Uuid>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PartSource {
    pub waypoint_id: String,
    pub document: Document,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredSource {
    mission_id: Uuid,
    schema_version: u32,
    revision: u64,
    title: String,
    objective: String,
    waypoints: Vec<StoredWaypoint>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredWaypoint {
    waypoint_id: String,
    title: String,
    prompt: String,
    part_file: String,
    #[serde(default)]
    depends_on: Vec<String>,
    #[serde(default)]
    validation: Vec<ValidationGate>,
    #[serde(default)]
    commit_checkpoint: CommitCheckpoint,
    #[serde(default)]
    non_goals: Vec<String>,
    #[serde(default)]
    ownership_refs: OwnershipRefs,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Document {
    #[serde(default)]
    pub blocks: Vec<Block>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Block {
    Heading {
        level: u8,
        inlines: Vec<Inline>,
    },
    Paragraph {
        inlines: Vec<Inline>,
    },
    BulletList {
        items: Vec<Vec<Block>>,
    },
    OrderedList {
        items: Vec<Vec<Block>>,
    },
    Blockquote {
        blocks: Vec<Block>,
    },
    CodeFence {
        language: String,
        text: String,
    },
    Table {
        header: Vec<Vec<Inline>>,
        rows: Vec<Vec<Vec<Inline>>>,
    },
    ThematicBreak,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Inline {
    Text { text: String },
    Emphasis { inlines: Vec<Inline> },
    Strong { inlines: Vec<Inline> },
    Code { text: String },
    Link { href: String, inlines: Vec<Inline> },
    HardBreak,
}

impl Default for CommitCheckpoint {
    fn default() -> Self {
        Self {
            message: String::new(),
        }
    }
}

impl MissionLifecycle {
    pub fn create_draft(
        &self,
        title: String,
        objective: String,
    ) -> Result<TypedDraft, MissionError> {
        let draft = TypedDraft {
            mission_id: Uuid::new_v4(),
            schema_version: TYPED_SOURCE_SCHEMA_VERSION,
            revision: 1,
            title,
            objective,
            waypoints: vec![],
        };
        self.save_typed(&draft, 0, false)?;
        Ok(draft)
    }

    pub fn save_draft(
        &self,
        mission_id: Uuid,
        mut draft: TypedDraft,
        expected_revision: u64,
    ) -> Result<TypedDraft, MissionError> {
        if draft.mission_id != mission_id {
            return Err(typed_error(
                "mission-id-mismatch",
                "draft mission_id must match the API target.",
            ));
        }
        draft.revision = expected_revision
            .checked_add(1)
            .ok_or_else(|| typed_error("revision-exhausted", "revision cannot be incremented."))?;
        self.save_typed(&draft, expected_revision, false)?;
        Ok(draft)
    }

    pub fn publish_draft(
        &self,
        mission_id: Uuid,
        mut draft: TypedDraft,
        expected_revision: u64,
    ) -> Result<TypedDraft, MissionError> {
        if draft.mission_id != mission_id {
            return Err(typed_error(
                "mission-id-mismatch",
                "draft mission_id must match the API target.",
            ));
        }
        draft.revision = expected_revision
            .checked_add(1)
            .ok_or_else(|| typed_error("revision-exhausted", "revision cannot be incremented."))?;
        validate_typed(&draft, true)?;
        let waypoint_ids: Vec<_> = draft
            .waypoints
            .iter()
            .map(|waypoint| waypoint.waypoint_id.clone())
            .collect();
        for (index, waypoint) in draft.waypoints.iter_mut().enumerate() {
            waypoint.depends_on = if index == 0 {
                vec![]
            } else {
                vec![waypoint_ids[index - 1].clone()]
            };
        }
        self.save_typed(&draft, expected_revision, true)?;
        Ok(draft)
    }

    pub fn get_mission_source(&self, mission_id: Uuid) -> Result<TypedDraft, MissionError> {
        let mission_dir = self.store.root.join(mission_id.to_string());
        let path = mission_dir.join("source.json");
        if !mission_dir.exists() || !path.exists() {
            return Err(MissionError::NotFound(mission_id));
        }
        ensure_safe_path(&self.store.root, &mission_dir)?;
        ensure_safe_path(&self.store.root, &path)?;
        let source = serde_json::from_slice::<StoredSource>(&fs::read(path)?).map_err(|_| {
            typed_error(
                "invalid-source-json",
                "source.json must be strict typed JSON.",
            )
        })?;
        let mut waypoints = Vec::with_capacity(source.waypoints.len());
        for waypoint in source.waypoints {
            if waypoint.part_file != format!("parts/{}.json", waypoint.waypoint_id) {
                return Err(typed_error(
                    "unsafe-part-path",
                    "Part paths are API-managed paths below the Mission root.",
                ));
            }
            let part_path = mission_dir.join(&waypoint.part_file);
            ensure_safe_path(&self.store.root, &mission_dir.join("parts"))?;
            ensure_safe_path(&self.store.root, &part_path)?;
            let part = serde_json::from_slice(&fs::read(part_path)?).map_err(|_| {
                typed_error("invalid-part-json", "Part files must be strict typed JSON.")
            })?;
            waypoints.push(TypedWaypoint {
                waypoint_id: waypoint.waypoint_id,
                title: waypoint.title,
                prompt: waypoint.prompt,
                part,
                depends_on: waypoint.depends_on,
                validation: waypoint.validation,
                commit_checkpoint: waypoint.commit_checkpoint,
                non_goals: waypoint.non_goals,
                ownership_refs: waypoint.ownership_refs,
            });
        }
        let draft = TypedDraft {
            mission_id: source.mission_id,
            schema_version: source.schema_version,
            revision: source.revision,
            title: source.title,
            objective: source.objective,
            waypoints,
        };
        validate_typed(&draft, false)?;
        Ok(draft)
    }

    pub fn render_mission(&self, mission_id: Uuid) -> Result<String, MissionError> {
        let source = self.get_mission_source(mission_id)?;
        let mut markdown = format!("# {}\n\n{}\n", source.title, source.objective);
        for waypoint in source.waypoints {
            markdown.push_str(&format!("\n## {}\n\n", waypoint.title));
            render_blocks(&mut markdown, &waypoint.part.document.blocks, 0);
        }
        Ok(markdown)
    }

    pub(crate) fn save_typed(
        &self,
        draft: &TypedDraft,
        expected: u64,
        publish: bool,
    ) -> Result<(), MissionError> {
        validate_typed(draft, publish)?;
        fs::create_dir_all(&self.store.root)?;
        let _lock = TypedLock::acquire(&self.store.root, draft.mission_id)?;
        let mission_dir = self.store.root.join(draft.mission_id.to_string());
        let current = match self.get_mission_source(draft.mission_id) {
            Ok(source) => Some(source),
            Err(MissionError::NotFound(_)) => None,
            Err(error) => return Err(error),
        };
        let actual = current.as_ref().map_or(0, |source| source.revision);
        if actual != expected {
            return Err(MissionError::StaleRevision { expected, actual });
        }
        let temp = self.store.root.join(format!(
            ".{}.typed.{}.tmp",
            draft.mission_id,
            Uuid::new_v4()
        ));
        fs::create_dir_all(temp.join("parts"))?;
        fs::create_dir_all(temp.join("history"))?;
        fs::write(
            temp.join("source.json"),
            serde_json::to_vec_pretty(&stored_source(draft))?,
        )?;
        for waypoint in &draft.waypoints {
            fs::write(
                temp.join("parts")
                    .join(format!("{}.json", waypoint.waypoint_id)),
                serde_json::to_vec_pretty(&waypoint.part)?,
            )?;
        }
        fs::write(
            temp.join("history")
                .join(format!("revision-{}.json", draft.revision)),
            serde_json::to_vec_pretty(&stored_source(draft))?,
        )?;
        if publish {
            fs::write(temp.join("mission.md"), self.render_typed(draft))?;
        }
        if mission_dir.exists() {
            copy_directory(&mission_dir, &temp)?;
            fs::write(
                temp.join("source.json"),
                serde_json::to_vec_pretty(&stored_source(draft))?,
            )?;
            fs::remove_dir_all(temp.join("parts"))?;
            fs::create_dir_all(temp.join("parts"))?;
            for waypoint in &draft.waypoints {
                fs::write(
                    temp.join("parts")
                        .join(format!("{}.json", waypoint.waypoint_id)),
                    serde_json::to_vec_pretty(&waypoint.part)?,
                )?;
            }
            fs::write(
                temp.join("history")
                    .join(format!("revision-{}.json", draft.revision)),
                serde_json::to_vec_pretty(&stored_source(draft))?,
            )?;
            if publish {
                fs::write(temp.join("mission.md"), self.render_typed(draft))?;
            } else if temp.join("mission.md").exists() {
                fs::remove_file(temp.join("mission.md"))?;
            }
        }
        let backup = self
            .store
            .root
            .join(format!(".{}.typed.backup", draft.mission_id));
        if backup.exists() {
            fs::remove_dir_all(&backup)?;
        }
        if mission_dir.exists() {
            fs::rename(&mission_dir, &backup)?;
        }
        if let Err(error) = fs::rename(&temp, &mission_dir) {
            if backup.exists() {
                fs::rename(&backup, &mission_dir)?;
            }
            return Err(error.into());
        }
        if backup.exists() {
            if let Err(error) = remove_typed_backup(&backup) {
                eprintln!(
                    "mission-api: accepted typed revision {} but retained recoverable backup {}: {error}",
                    draft.revision,
                    backup.display()
                );
            }
        }
        Ok(())
    }

    fn render_typed(&self, draft: &TypedDraft) -> String {
        let mut output = format!("# {}\n\n{}\n", draft.title, draft.objective);
        for waypoint in &draft.waypoints {
            output.push_str(&format!("\n## {}\n\n", waypoint.title));
            render_blocks(&mut output, &waypoint.part.document.blocks, 0);
        }
        output
    }
}

fn remove_typed_backup(path: &Path) -> std::io::Result<()> {
    #[cfg(test)]
    if TYPED_BACKUP_CLEANUP_FAILURE.load(std::sync::atomic::Ordering::SeqCst) {
        return Err(std::io::Error::other(
            "injected typed backup cleanup failure",
        ));
    }
    fs::remove_dir_all(path)
}

#[cfg(test)]
static TYPED_BACKUP_CLEANUP_FAILURE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

struct TypedLock {
    path: PathBuf,
}

impl TypedLock {
    fn acquire(root: &Path, mission_id: Uuid) -> Result<Self, MissionError> {
        let path = root.join(format!(".{mission_id}.typed.lock"));
        loop {
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    std::thread::yield_now();
                }
                Err(error) => return Err(error.into()),
            }
        }
    }
}

impl Drop for TypedLock {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir(&self.path) {
            eprintln!(
                "mission-api: retained typed write lock {} after completed write: {error}",
                self.path.display()
            );
        }
    }
}

fn ensure_safe_path(root: &Path, path: &Path) -> Result<(), MissionError> {
    let relative = path.strip_prefix(root).map_err(|_| {
        typed_error(
            "unsafe-part-path",
            "managed paths must remain below the Mission root.",
        )
    })?;
    let mut current = root.to_path_buf();
    let root_metadata = fs::symlink_metadata(&current).map_err(MissionError::Io)?;
    if root_metadata.file_type().is_symlink() {
        return Err(typed_error(
            "unsafe-part-path",
            "managed source paths and their ancestors must not be symlinks.",
        ));
    }
    for component in relative.components() {
        current.push(component);
        let metadata = fs::symlink_metadata(&current).map_err(MissionError::Io)?;
        if metadata.file_type().is_symlink() {
            return Err(typed_error(
                "unsafe-part-path",
                "managed source paths and their ancestors must not be symlinks.",
            ));
        }
    }
    Ok(())
}

fn stored_source(draft: &TypedDraft) -> StoredSource {
    StoredSource {
        mission_id: draft.mission_id,
        schema_version: draft.schema_version,
        revision: draft.revision,
        title: draft.title.clone(),
        objective: draft.objective.clone(),
        waypoints: draft
            .waypoints
            .iter()
            .map(|waypoint| StoredWaypoint {
                waypoint_id: waypoint.waypoint_id.clone(),
                title: waypoint.title.clone(),
                prompt: waypoint.prompt.clone(),
                part_file: format!("parts/{}.json", waypoint.waypoint_id),
                depends_on: waypoint.depends_on.clone(),
                validation: waypoint.validation.clone(),
                commit_checkpoint: waypoint.commit_checkpoint.clone(),
                non_goals: waypoint.non_goals.clone(),
                ownership_refs: waypoint.ownership_refs.clone(),
            })
            .collect(),
    }
}

fn typed_error(code: &'static str, message: &str) -> MissionError {
    MissionError::Validation(
        1,
        vec![Diagnostic {
            code,
            source: "source.json".into(),
            pointer: String::new(),
            waypoint_id: None,
            message: message.into(),
        }],
    )
}

fn validate_typed(draft: &TypedDraft, publishing: bool) -> Result<(), MissionError> {
    if draft.schema_version != TYPED_SOURCE_SCHEMA_VERSION {
        return Err(typed_error(
            "invalid-schema-version",
            "source schema_version must be 2.",
        ));
    }
    let mut ids = BTreeSet::new();
    if publishing && draft.waypoints.is_empty() {
        return Err(typed_error(
            "draft-not-ready",
            "publishing requires an ordered waypoint set.",
        ));
    }
    for waypoint in &draft.waypoints {
        if waypoint.waypoint_id.is_empty()
            || waypoint.waypoint_id.contains('/')
            || waypoint.waypoint_id.contains('\\')
            || waypoint.waypoint_id == "."
            || waypoint.waypoint_id == ".."
        {
            return Err(typed_error(
                "invalid-waypoint-id",
                "waypoint IDs must be safe path components.",
            ));
        }
        if !ids.insert(&waypoint.waypoint_id) || waypoint.part.waypoint_id != waypoint.waypoint_id {
            return Err(typed_error(
                "invalid-part-reference",
                "each waypoint needs one matching unique managed Part.",
            ));
        }
        if waypoint
            .non_goals
            .iter()
            .any(|value| value.trim().is_empty())
        {
            return Err(typed_error(
                "invalid-non-goal",
                "non_goals entries must be nonblank.",
            ));
        }
        validate_blocks(&waypoint.part.document.blocks)?;
        if publishing
            && (waypoint.validation.is_empty()
                || waypoint
                    .validation
                    .iter()
                    .any(|gate| gate.command.trim().is_empty())
                || waypoint.commit_checkpoint.message.trim().is_empty())
        {
            return Err(typed_error(
                "draft-not-ready",
                "publishing requires a nonblank validation command and checkpoint per waypoint.",
            ));
        }
        if waypoint
            .validation
            .iter()
            .any(|gate| gate.cwd.as_ref().is_some_and(|cwd| cwd.trim().is_empty()))
        {
            return Err(typed_error(
                "invalid-validation-cwd",
                "a supplied execution cwd must be nonblank.",
            ));
        }
    }
    for waypoint in &draft.waypoints {
        let mut dependencies = BTreeSet::new();
        for dependency in &waypoint.depends_on {
            if dependency == &waypoint.waypoint_id
                || !ids.contains(dependency)
                || !dependencies.insert(dependency)
            {
                return Err(typed_error(
                    "invalid-dependency-reference",
                    "dependencies must be unique references to other waypoints.",
                ));
            }
        }
    }
    Ok(())
}

fn validate_blocks(blocks: &[Block]) -> Result<(), MissionError> {
    for block in blocks {
        match block {
            Block::Heading { level, inlines } if *level == 0 || *level > 6 => {
                return Err(typed_error(
                    "invalid-heading-level",
                    "heading levels must be 1 through 6.",
                ));
            }
            Block::Heading { inlines, .. } | Block::Paragraph { inlines } => {
                validate_inlines(inlines)?
            }
            Block::BulletList { items } | Block::OrderedList { items } => {
                for item in items {
                    validate_blocks(item)?;
                }
            }
            Block::Blockquote { blocks } => validate_blocks(blocks)?,
            Block::Table { header, rows } => {
                for row in header {
                    validate_inlines(row)?;
                }
                for row in rows {
                    for cell in row {
                        validate_inlines(cell)?;
                    }
                }
            }
            Block::CodeFence { .. } | Block::ThematicBreak => {}
        }
    }
    Ok(())
}
fn validate_inlines(inlines: &[Inline]) -> Result<(), MissionError> {
    for inline in inlines {
        match inline {
            Inline::Emphasis { inlines }
            | Inline::Strong { inlines }
            | Inline::Link { inlines, .. } => validate_inlines(inlines)?,
            _ => {}
        }
    }
    Ok(())
}
fn render_blocks(out: &mut String, blocks: &[Block], depth: usize) {
    for block in blocks {
        match block {
            Block::Heading { level, inlines } => {
                out.push_str(&"#".repeat(*level as usize));
                out.push(' ');
                render_inlines(out, inlines);
                out.push_str("\n\n");
            }
            Block::Paragraph { inlines } => {
                render_inlines(out, inlines);
                out.push_str("\n\n");
            }
            Block::BulletList { items } | Block::OrderedList { items } => {
                for (i, item) in items.iter().enumerate() {
                    out.push_str(&"  ".repeat(depth));
                    if matches!(block, Block::OrderedList { .. }) {
                        out.push_str(&format!("{}. ", i + 1));
                    } else {
                        out.push_str("- ");
                    }
                    render_blocks(out, item, depth + 1);
                }
            }
            Block::Blockquote { blocks } => {
                let mut nested = String::new();
                render_blocks(&mut nested, blocks, depth);
                for line in nested.lines() {
                    out.push_str("> ");
                    out.push_str(line);
                    out.push('\n');
                }
            }
            Block::CodeFence { language, text } => {
                out.push_str("```");
                out.push_str(language);
                out.push('\n');
                out.push_str(text);
                out.push_str("\n```\n\n");
            }
            Block::Table { header, rows } => {
                for row in std::iter::once(header).chain(rows.iter()) {
                    out.push('|');
                    for cell in row {
                        render_inlines(out, cell);
                        out.push('|');
                    }
                    out.push('\n');
                }
            }
            Block::ThematicBreak => out.push_str("---\n\n"),
        }
    }
}
fn render_inlines(out: &mut String, inlines: &[Inline]) {
    for inline in inlines {
        match inline {
            Inline::Text { text } => out.push_str(text),
            Inline::Code { text } => {
                out.push('`');
                out.push_str(text);
                out.push('`');
            }
            Inline::HardBreak => out.push_str("  \n"),
            Inline::Emphasis { inlines } => {
                out.push('*');
                render_inlines(out, inlines);
                out.push('*');
            }
            Inline::Strong { inlines } => {
                out.push_str("**");
                render_inlines(out, inlines);
                out.push_str("**");
            }
            Inline::Link { href, inlines } => {
                out.push('[');
                render_inlines(out, inlines);
                out.push_str("](");
                out.push_str(href);
                out.push(')');
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn accepted_write_survives_recoverable_backup_cleanup_failure() {
        let directory = tempdir().unwrap();
        let lifecycle = MissionLifecycle::open(directory.path());
        let created = lifecycle
            .create_draft("initial".into(), "objective".into())
            .unwrap();
        let mut replacement = created.clone();
        replacement.title = "accepted replacement".into();

        TYPED_BACKUP_CLEANUP_FAILURE.store(true, std::sync::atomic::Ordering::SeqCst);
        let accepted = lifecycle
            .save_draft(created.mission_id, replacement, 1)
            .unwrap();
        TYPED_BACKUP_CLEANUP_FAILURE.store(false, std::sync::atomic::Ordering::SeqCst);

        assert_eq!(accepted.revision, 2);
        assert_eq!(
            lifecycle.get_mission_source(created.mission_id).unwrap(),
            accepted
        );
        assert!(
            directory
                .path()
                .join(".workflow-tools/mission/missions")
                .join(format!(".{}.typed.backup", created.mission_id))
                .is_dir(),
            "the old revision remains recoverable after cleanup warning"
        );
    }
}
