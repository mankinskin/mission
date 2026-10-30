use super::*;

#[test]
fn migration_accepts_case_variant_headings_and_preserves_authored_part_content() {
    let workspace = TempDir::new().unwrap();
    let contents = roadmap("Case Variant Mission", &[("Alpha", "01-first.md", None)])
        .replace("## Outcome Summary", "## Outcome summary")
        .replace(
            "## Roadmap Waypoints",
            "## Validation gates\n\n- `cargo test -p first`\n- `cargo test -p second`\n\n## Roadmap Waypoints",
        );
    let dossier = write_dossier(workspace.path(), "case-variant", &contents);
    fs::write(
        dossier.join("01-first.md"),
        "## Outcome\n\nPreserve part content.\n\n## Expected state\n\nAuthored state remains intact.\n",
    )
    .unwrap();

    let result = execute_cli(
        workspace.path(),
        Operation::MigrateDossiers {
            dossier_path: Some(dossier),
            dry_run: false,
        },
    );
    assert_eq!(result.status, "ok");
    let report = result.migration_report.unwrap();
    assert_eq!(report.migrated, 1, "{:?}", report.records);
    let accepted = MissionStore::open(workspace.path())
        .get(report.records[0].mission_id.unwrap())
        .unwrap();
    assert_eq!(
        accepted.bundle.manifest.objective,
        "Preserve this legacy roadmap."
    );
    assert_eq!(
        accepted
            .bundle
            .manifest
            .validation_gates
            .iter()
            .map(|gate| gate.command.as_str())
            .collect::<Vec<_>>(),
        ["cargo test -p first", "cargo test -p second"]
    );
    assert!(
        accepted.bundle.waypoints[0]
            .part_markdown
            .contains("## Expected state\n\nAuthored state remains intact.")
    );
}

#[test]
fn migration_rejects_missing_or_empty_outcome_summary() {
    for (name, contents) in [
        (
            "missing-outcome",
            roadmap("Missing Outcome", &[("Alpha", "01-first.md", None)]).replace(
                "## Outcome Summary\n\nPreserve this legacy roadmap.\n\n",
                "",
            ),
        ),
        (
            "empty-outcome",
            roadmap("Empty Outcome", &[("Alpha", "01-first.md", None)])
                .replace("Preserve this legacy roadmap.", "   "),
        ),
    ] {
        let workspace = TempDir::new().unwrap();
        let dossier = write_dossier(workspace.path(), name, &contents);
        let result = execute_cli(
            workspace.path(),
            Operation::MigrateDossiers {
                dossier_path: Some(dossier),
                dry_run: true,
            },
        );
        assert_eq!(result.status, "ok");
        let report = result.migration_report.unwrap();
        assert_eq!(report.blocked, 1, "{:?}", report.records);
        assert!(
            report.records[0]
                .reason
                .as_deref()
                .unwrap()
                .contains("non-empty Outcome Summary"),
            "{:?}",
            report.records
        );
    }
}

#[test]
fn migration_accepts_authored_schema_aliases_without_synthesizing_metadata() {
    let workspace = TempDir::new().unwrap();
    let dossier = write_dossier(workspace.path(), "authored-schema", &authored_roadmap());

    let result = execute_cli(
        workspace.path(),
        Operation::MigrateDossiers {
            dossier_path: Some(dossier),
            dry_run: false,
        },
    );

    assert_eq!(result.status, "ok");
    let report = result.migration_report.unwrap();
    assert_eq!(report.migrated, 1, "{:?}", report.records);
    let accepted = MissionStore::open(workspace.path())
        .get(report.records[0].mission_id.unwrap())
        .unwrap();
    assert_eq!(
        accepted.bundle.manifest.objective,
        "Preserve the authoring contract without adding execution metadata."
    );
    assert_eq!(accepted.bundle.waypoints[0].mode, WaypointMode::Research);
    assert_eq!(
        accepted.bundle.waypoints[1].depends_on,
        ["inspect-the-current-state"]
    );
}

#[test]
fn migration_accepts_authored_waypoints_with_an_overview_index_table() {
    let workspace = TempDir::new().unwrap();
    let roadmap = authored_roadmap().replace(
        "## Roadmap Waypoints",
        "| Waypoint | Package | Depends on | Status |\n|---|---|---|---|\n| W1 | inspect-current-state | — | done |\n| W2 | apply-change | W1 | pending |\n\n## Roadmap Waypoints",
    );
    let dossier = write_dossier(workspace.path(), "authored-with-index", &roadmap);

    let result = execute_cli(
        workspace.path(),
        Operation::MigrateDossiers {
            dossier_path: Some(dossier),
            dry_run: false,
        },
    );

    assert_eq!(result.status, "ok");
    let report = result.migration_report.unwrap();
    assert_eq!(report.migrated, 1, "{:?}", report.records);
}

#[test]
fn migration_explains_unsupported_legacy_waypoint_index_tables() {
    let workspace = TempDir::new().unwrap();
    let dossier = write_dossier(
        workspace.path(),
        "legacy-index",
        "# ROADMAP — legacy index\n\nOutcome: Preserve legacy content.\n\n| Waypoint | Package | Depends on | Status |\n|---|---|---|---|\n| W1 | package | — | done |\n",
    );

    let result = execute_cli(
        workspace.path(),
        Operation::MigrateDossiers {
            dossier_path: Some(dossier),
            dry_run: true,
        },
    );

    assert_eq!(result.status, "ok");
    let report = result.migration_report.unwrap();
    assert_eq!(report.blocked, 1, "{:?}", report.records);
    assert!(
        report.records[0]
            .reason
            .as_deref()
            .unwrap()
            .contains("legacy waypoint index table"),
        "{:?}",
        report.records
    );
}
