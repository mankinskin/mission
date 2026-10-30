use super::*;

pub(crate) fn fixture() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(".w3-fixture-")
        .tempdir_in(env!("CARGO_MANIFEST_DIR"))
        .unwrap()
}

pub(crate) fn mission(workspace: &Path) -> AcceptedMission {
    MissionLifecycle::open(workspace)
        .create(
            "Mission".into(),
            "Objective".into(),
            vec![DirectWaypoint {
                waypoint_id: "verify".into(),
                title: "Verify".into(),
                depends_on: vec![],
                prompt: "Check".into(),
                validation: vec![ValidationGate {
                    command: "cargo test".into(),
                    cwd: None,
                }],
                commit_checkpoint: CommitCheckpoint {
                    message: "Verify".into(),
                },
                non_goals: vec![],
            }],
        )
        .unwrap()
}

pub(crate) fn spec(workspace: &Path, v2: bool) -> (SpecManifest, CriterionReference) {
    let mut spec = SpecManifest::new("mission/contract", "Canonical specification", "mission");
    if v2 {
        spec.extra.insert("format_version".into(), json!(2));
        spec.extra.insert(
            "criteria".into(),
            json!([CriterionArtifact {
                criterion_id: "schema-check".into(),
                owner_component_id: "mission".into(),
                statement: "Authoritative <criterion> | [text]".into(),
                measurement: None,
                evidence_ids: vec![],
            }]),
        );
    } else {
        spec.extra.insert(
            "acceptance_criteria".into(),
            json!([AcceptanceCriterion {
                id: "schema-check".into(),
                statement: "Authoritative <criterion> | [text]".into(),
                expected_property_ids: vec![],
                required_evidence_ids: vec![],
            }]),
        );
    }
    write_spec(workspace, &spec);
    let reference = CriterionReference {
        specification_id: spec.id,
        criterion_id: CriterionId::new("schema-check").unwrap(),
    };
    (spec, reference)
}

pub(crate) fn spec_path(workspace: &Path, id: Uuid) -> PathBuf {
    workspace
        .join(".workflow-tools/spec/specs")
        .join(id.to_string())
        .join("spec.toml")
}

pub(crate) fn write_spec(workspace: &Path, spec: &SpecManifest) {
    let path = spec_path(workspace, spec.id);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, toml::to_string(spec).unwrap()).unwrap();
}

pub(crate) fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn walk(root: &Path, path: &Path, values: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, values);
            } else {
                values.insert(
                    path.strip_prefix(root).unwrap().to_path_buf(),
                    fs::read(path).unwrap(),
                );
            }
        }
    }
    let mut values = BTreeMap::new();
    walk(root, root, &mut values);
    values
}

pub(crate) fn requirement(reference: CriterionReference) -> Requirement {
    Requirement {
        id: "mission-contract".into(),
        text: "Canonical reference, not copied criteria".into(),
        deferred: false,
        canonical_criteria: vec![reference],
    }
}
