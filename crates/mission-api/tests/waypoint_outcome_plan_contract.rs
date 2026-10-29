use mission_api::*;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

fn fixture() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(".w7-")
        .tempdir_in(env!("CARGO_MANIFEST_DIR"))
        .unwrap()
}

fn legacy(workspace: &Path) -> AcceptedMission {
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

fn plan() -> Value {
    json!({"expected_outcome":"Ready <safe> | [text]",
        "alternative_outcomes":[{"outcome":"Not ready *yet*", "acceptable_handling":["Inspect <failure>", "Replan [manually]"]}]})
}

fn planned(accepted: &AcceptedMission) -> MissionBundle {
    let mut value = serde_json::to_value(&accepted.bundle).unwrap();
    value["manifest"]["schema_version"] = json!(2);
    value["waypoints"][0]["outcome_plan"] = plan();
    serde_json::from_value(value).expect("native v2 must represent complete planned outcomes")
}

fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn walk(root: &Path, path: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                out.insert(
                    path.strip_prefix(root).unwrap().to_path_buf(),
                    fs::read(path).unwrap(),
                );
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

#[test]
fn native_v2_represents_validates_and_renders_planned_outcomes() {
    let workspace = fixture();
    let accepted = legacy(workspace.path());
    let bundle = planned(&accepted);
    assert!(validate_bundle(&bundle).is_ok());
    let rendered = render(&bundle, accepted.mission_id).unwrap();
    let roadmap = String::from_utf8(rendered.roadmap.clone()).unwrap();
    assert!(roadmap.contains("Expected outcome"));
    assert!(roadmap.contains("&lt;safe&gt;"));
    assert!(roadmap.contains(r"\| \[text\]"));
    assert!(roadmap.contains(r"Not ready \*yet\*"));
    assert!(roadmap.contains("manual guidance"));
    let part = String::from_utf8(rendered.parts["waypoints/01-verify.md"].clone()).unwrap();
    assert!(part.contains("Acceptable handling"));
    assert!(
        part.find("Inspect &lt;failure&gt;").unwrap() < part.find(r"Replan \[manually\]").unwrap()
    );
    assert_eq!(rendered, render(&bundle, accepted.mission_id).unwrap());
    let mut invalid_result = bundle;
    invalid_result
        .manifest
        .operator_results
        .push(OperatorResult {
            waypoint_id: "verify".into(),
            outcome: "Not ready *yet*".into(),
            operator_command: "manual check".into(),
            evidence_pointer: "evidence/log".into(),
        });
    assert!(
        validate_bundle(&invalid_result)
            .unwrap_err()
            .iter()
            .any(|d| d.code == "invalid-operator-result")
    );
}

#[test]
fn legacy_bytes_and_rendering_remain_unchanged_without_invented_plans() {
    let workspace = fixture();
    let accepted = legacy(workspace.path());
    let before = serde_json::to_value(&accepted.bundle).unwrap();
    assert!(before["waypoints"][0].get("outcome_plan").is_none());
    let rendered = render(&accepted.bundle, accepted.mission_id).unwrap();
    let mut value = before.clone();
    value["waypoints"][0]["outcome_plan"] = Value::Null;
    let decoded = serde_json::from_value::<MissionBundle>(value).unwrap();
    assert_eq!(serde_json::to_value(&decoded).unwrap(), before);
    assert_eq!(rendered, render(&decoded, accepted.mission_id).unwrap());
    assert!(
        !String::from_utf8(rendered.roadmap)
            .unwrap()
            .contains("Planned outcomes")
    );
    let mut journal = MissionLifecycle::plan_migration(&[workspace.path().to_path_buf()]).unwrap();
    MissionLifecycle::apply_migration(&mut journal).unwrap();
    assert_eq!(journal.records[0].state, MigrationState::Applied);
    let source = MissionLifecycle::open(workspace.path())
        .get_mission_source(accepted.mission_id)
        .unwrap();
    assert_eq!(source.schema_version, 2);
    assert!(source.waypoints[0].outcome_plan.is_none());
    assert!(
        serde_json::to_value(source).unwrap()["waypoints"][0]
            .get("outcome_plan")
            .is_none()
    );
}

#[test]
fn invalid_native_plans_and_versions_preserve_all_accepted_and_generated_bytes() {
    let workspace = fixture();
    let accepted = legacy(workspace.path());
    let store = MissionStore::open(workspace.path());
    let mut bundle = planned(&accepted);
    bundle.manifest.revision = 2;
    store
        .publish(accepted.mission_id, bundle.clone(), 1)
        .unwrap();
    let before = snapshot(workspace.path());
    let valid = serde_json::to_value(bundle).unwrap();
    let mut cases = vec![
        Value::Null,
        json!({}),
        json!({"expected_outcome":"ready"}),
        json!({"expected_outcome":"ready","alternative_outcomes":[]}),
        json!({"expected_outcome":" ","alternative_outcomes":plan()["alternative_outcomes"]}),
        json!({"expected_outcome":"ready","alternative_outcomes":[{"outcome":" ", "acceptable_handling":["inspect"]}]}),
        json!({"expected_outcome":"ready","alternative_outcomes":[{"outcome":"failed", "acceptable_handling":[]}]}),
        json!({"expected_outcome":"ready","alternative_outcomes":[{"outcome":"failed", "acceptable_handling":[" "]}]}),
        json!({"expected_outcome":"ready","alternative_outcomes":[{"outcome":"failed", "acceptable_handling":"inspect"}]}),
    ];
    for bad_plan in cases.drain(..) {
        let mut bad = valid.clone();
        bad["manifest"]["revision"] = json!(3);
        bad["waypoints"][0]["outcome_plan"] = bad_plan;
        if let Ok(bad) = serde_json::from_value::<MissionBundle>(bad) {
            assert!(
                validate_bundle(&bad)
                    .unwrap_err()
                    .iter()
                    .any(|d| d.code == "invalid-outcome-plan")
            );
            assert!(store.publish(accepted.mission_id, bad, 2).is_err());
        }
        assert_eq!(snapshot(workspace.path()), before);
    }
    for version in [0, 3, 99] {
        let mut bad = valid.clone();
        bad["manifest"]["schema_version"] = json!(version);
        let bad: MissionBundle = serde_json::from_value(bad).unwrap();
        assert!(
            validate_bundle(&bad)
                .unwrap_err()
                .iter()
                .any(|d| d.code == "unsupported-schema-version")
        );
        assert!(store.import(accepted.mission_id, bad, 2).is_err());
        assert_eq!(snapshot(workspace.path()), before);
    }
}

#[test]
fn native_v2_migrates_losslessly_and_typed_v3_bridges_history_results_and_provenance() {
    let workspace = fixture();
    fs::create_dir(workspace.path().join("evidence")).unwrap();
    fs::write(workspace.path().join("evidence/log"), "Actual observation").unwrap();
    let accepted = legacy(workspace.path());
    let store = MissionStore::open(workspace.path());
    let mut bundle = planned(&accepted);
    bundle.manifest.revision = 2;
    bundle.manifest.operator_results.push(OperatorResult {
        waypoint_id: "verify".into(),
        outcome: "passed".into(),
        operator_command: "manual check".into(),
        evidence_pointer: "evidence/log".into(),
    });
    store
        .publish(accepted.mission_id, bundle.clone(), 1)
        .unwrap();
    let history = fs::read(
        workspace
            .path()
            .join(".workflow-tools/mission/missions")
            .join(accepted.mission_id.to_string())
            .join("history/states/revision-2/accepted.json"),
    )
    .unwrap();
    let mut journal = MissionLifecycle::plan_migration(&[workspace.path().to_path_buf()]).unwrap();
    MissionLifecycle::apply_migration(&mut journal).unwrap();
    assert_eq!(journal.records[0].state, MigrationState::Applied);
    let lifecycle = MissionLifecycle::open(workspace.path());
    let source = lifecycle.get_mission_source(accepted.mission_id).unwrap();
    assert_eq!(source.schema_version, 3);
    assert_eq!(
        serde_json::to_value(&source.waypoints[0].outcome_plan).unwrap(),
        plan()
    );
    assert_eq!(
        SharedEntity::from_typed_mission(&source)
            .unwrap()
            .source_schema,
        SharedSourceSchema::MissionSourceV3
    );
    let published = lifecycle
        .publish_draft(accepted.mission_id, source, 3)
        .unwrap();
    let current = store.get(accepted.mission_id).unwrap();
    assert_eq!(current.bundle.manifest.schema_version, 2);
    assert_eq!(
        current.bundle.manifest.operator_results,
        bundle.manifest.operator_results
    );
    assert_eq!(
        current.bundle.waypoints[0].depends_on,
        bundle.waypoints[0].depends_on
    );
    assert_eq!(current.bundle.waypoints[0].state, bundle.waypoints[0].state);
    assert_eq!(
        current.bundle.waypoints[0].outcome_plan,
        published.waypoints[0].outcome_plan
    );
    assert_eq!(
        SharedEntity::from_mission(&current).unwrap().source_schema,
        SharedSourceSchema::MissionBundleV2
    );
    let root = workspace
        .path()
        .join(".workflow-tools/mission/missions")
        .join(accepted.mission_id.to_string());
    assert_eq!(
        fs::read(root.join("history/states/revision-2/accepted.json")).unwrap(),
        history
    );
    let contract: Value =
        serde_json::from_slice(&fs::read(root.join("output-contract.json")).unwrap()).unwrap();
    assert_eq!(contract["renderer"], "mission-api/6");
    let roadmap = fs::read_to_string(root.join("generated/ROADMAP.md")).unwrap();
    assert!(roadmap.contains("mission-api/6"));
    assert!(roadmap.contains("manual check"));
    assert!(roadmap.contains("Ready &lt;safe&gt;"));
    assert!(
        lifecycle
            .render_mission(accepted.mission_id)
            .unwrap()
            .contains("Acceptable handling")
    );
    let next = lifecycle
        .edit_objective(accepted.mission_id, "New objective".into(), 4)
        .unwrap();
    assert_eq!(next.bundle.manifest.schema_version, 2);
    assert_eq!(
        lifecycle
            .get_mission_source(accepted.mission_id)
            .unwrap()
            .schema_version,
        3
    );
}

#[test]
fn typed_v3_invalid_and_unsupported_writes_preserve_source_and_outputs() {
    let workspace = fixture();
    let lifecycle = MissionLifecycle::open(workspace.path());
    let draft = lifecycle
        .create_draft("Draft".into(), "Objective".into())
        .unwrap();
    let mut valid = serde_json::to_value(&draft).unwrap();
    valid["schema_version"] = json!(3);
    valid["waypoints"] = json!([{
        "waypoint_id":"verify", "title":"Verify", "prompt":"Check",
        "part":{"waypoint_id":"verify","document":{"blocks":[]}},
        "validation":[{"command":"check"}], "commit_checkpoint":{"message":"checkpoint"},
        "outcome_plan":plan()
    }]);
    let valid: TypedDraft = serde_json::from_value(valid).unwrap();
    let source = lifecycle.publish_draft(draft.mission_id, valid, 1).unwrap();
    let before = snapshot(workspace.path());
    for version in [0, 1, 4, 99] {
        let mut invalid = source.clone();
        invalid.schema_version = version;
        assert!(lifecycle.save_draft(draft.mission_id, invalid, 2).is_err());
        assert_eq!(snapshot(workspace.path()), before);
    }
    for invalid_plan in [
        None,
        Some(WaypointOutcomePlan {
            expected_outcome: " ".into(),
            alternative_outcomes: vec![],
        }),
    ] {
        let mut invalid = source.clone();
        invalid.waypoints[0].outcome_plan = invalid_plan;
        assert!(
            lifecycle
                .publish_draft(draft.mission_id, invalid, 2)
                .is_err()
        );
        assert_eq!(snapshot(workspace.path()), before);
    }
    let mut legacy_with_plan = source;
    legacy_with_plan.schema_version = 2;
    assert!(
        lifecycle
            .save_draft(draft.mission_id, legacy_with_plan, 2)
            .is_err()
    );
    assert_eq!(snapshot(workspace.path()), before);
    let schema = serde_json::to_value(schemars::schema_for!(TypedDraft))
        .unwrap()
        .to_string();
    assert!(schema.contains("outcome_plan") && schema.contains("acceptable_handling"));
}

#[test]
fn repair_candidate_keeps_typed_v3_plan_and_does_not_coerce_version() {
    let workspace = fixture();
    let accepted = legacy(workspace.path());
    let mut bundle = planned(&accepted);
    bundle.manifest.revision = 2;
    MissionStore::open(workspace.path())
        .publish(accepted.mission_id, bundle, 1)
        .unwrap();
    let mut journal = MissionLifecycle::plan_migration(&[workspace.path().to_path_buf()]).unwrap();
    MissionLifecycle::apply_migration(&mut journal).unwrap();
    let root = workspace
        .path()
        .join(".workflow-tools/mission/missions")
        .join(accepted.mission_id.to_string());
    let mut source: Value =
        serde_json::from_slice(&fs::read(root.join("source.json")).unwrap()).unwrap();
    source["revision"] = json!(2);
    fs::write(
        root.join("source.json"),
        serde_json::to_vec_pretty(&source).unwrap(),
    )
    .unwrap();
    let lifecycle = MissionLifecycle::open(workspace.path());
    let mut request = RepairRequest {
        source_root: workspace.path().to_path_buf(),
        policy: REPAIR_POLICY.into(),
        records: vec![RepairRecordRequest {
            mission_id: accepted.mission_id,
            expected_revision: 3,
            accepted_sha256: String::new(),
            source_sha256: String::new(),
            payload_fingerprint: String::new(),
        }],
    };
    let discovery = lifecycle
        .repair_preview(workspace.path(), request.clone())
        .unwrap();
    let observed = &discovery.records[0];
    request.records[0].accepted_sha256 = observed.observed_accepted_sha256.clone().unwrap();
    request.records[0].source_sha256 = observed.observed_source_sha256.clone().unwrap();
    request.records[0].payload_fingerprint = observed.payload_fingerprint.clone().unwrap();
    let before = snapshot(workspace.path());
    let preview = lifecycle
        .repair_preview(workspace.path(), request.clone())
        .unwrap();
    assert_eq!(preview.status, "ready", "{preview:?}");
    let candidate = &preview.candidate_overlay[&accepted.mission_id.to_string()];
    assert_eq!(candidate.typed_source.schema_version, 3);
    assert_eq!(
        serde_json::to_value(&candidate.typed_source.waypoints[0].outcome_plan).unwrap(),
        plan()
    );
    assert_eq!(snapshot(workspace.path()), before);
    let applied = lifecycle
        .repair_apply(
            workspace.path(),
            request,
            preview.planned_journal_identity.as_deref().unwrap(),
        )
        .unwrap();
    assert_eq!(applied.status, "applied", "{applied:?}");
    let repaired = lifecycle.get_mission_source(accepted.mission_id).unwrap();
    assert_eq!(repaired.schema_version, 3);
    assert_eq!(
        serde_json::to_value(repaired.waypoints[0].outcome_plan.clone()).unwrap(),
        plan()
    );
    let rolled_back = lifecycle
        .repair_rollback(workspace.path(), applied.actual_journal_id.unwrap())
        .unwrap();
    assert_eq!(rolled_back.status, "rolled-back");
    let restored = lifecycle.get_mission_source(accepted.mission_id).unwrap();
    assert_eq!(restored.schema_version, 3);
    assert_eq!(
        serde_json::to_value(restored.waypoints[0].outcome_plan.clone()).unwrap(),
        plan()
    );
}

#[test]
fn malformed_plan_or_unsupported_native_migration_preserves_source_inventory() {
    let workspace = fixture();
    let accepted = legacy(workspace.path());
    let store = MissionStore::open(workspace.path());
    let mut bundle = planned(&accepted);
    bundle.manifest.revision = 2;
    store.publish(accepted.mission_id, bundle, 1).unwrap();
    let path = workspace
        .path()
        .join(".workflow-tools/mission/missions")
        .join(accepted.mission_id.to_string())
        .join("accepted.json");
    let valid: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    for (version, outcome) in [
        (2, Value::Null),
        (2, json!({"expected_outcome":"ready"})),
        (99, plan()),
    ] {
        let mut invalid = valid.clone();
        invalid["bundle"]["manifest"]["schema_version"] = json!(version);
        invalid["bundle"]["waypoints"][0]["outcome_plan"] = outcome;
        fs::write(&path, serde_json::to_vec_pretty(&invalid).unwrap()).unwrap();
        let before = snapshot(workspace.path());
        assert!(MissionLifecycle::plan_migration(&[workspace.path().to_path_buf()]).is_err());
        assert_eq!(snapshot(workspace.path()), before);
    }
}
