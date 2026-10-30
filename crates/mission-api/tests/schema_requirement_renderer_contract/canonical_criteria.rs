use super::*;

#[test]
fn requirement_accepts_typed_canonical_criterion_references() {
    let value = json!({
        "id": "mission-contract",
        "text": "Use the canonical criterion",
        "canonical_criteria": [{
            "specification_id": Uuid::new_v4(),
            "criterion_id": "mission-schema"
        }]
    });
    assert!(
        serde_json::from_value::<Requirement>(value).is_ok(),
        "Requirements must accept typed canonical specification/criterion IDs"
    );
}

#[test]
fn shared_projection_enumerates_only_proven_fields_from_actual_models() {
    let workspace = fixture();
    let accepted = mission(workspace.path());
    let ticket: TicketManifest = serde_json::from_value(json!({
        "id": accepted.mission_id,
        "created_at": "2026-10-29T00:00:00Z",
        "title": accepted.bundle.manifest.title,
        "state": "in-review",
        "plan_revision": 7,
        "foreign_attribute": {"retain": true}
    }))
    .unwrap();
    let before = serde_json::to_value(&ticket).unwrap();
    let projected_ticket = SharedEntity::from_ticket(&ticket).unwrap();
    let projected_mission = SharedEntity::from_mission(&accepted).unwrap();
    assert_eq!(projected_ticket.id, projected_mission.id);
    assert_eq!(projected_ticket.title, projected_mission.title);
    assert_eq!(
        projected_ticket.source_schema,
        SharedSourceSchema::TicketUnversioned
    );
    assert_eq!(
        projected_mission.source_schema,
        SharedSourceSchema::MissionBundleV1
    );
    let mut keys = serde_json::to_value(projected_ticket)
        .unwrap()
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    keys.sort();
    assert_eq!(keys, ["id", "schema_version", "source_schema", "title"]);
    assert_eq!(serde_json::to_value(ticket).unwrap(), before);
}

#[test]
fn shared_projection_strictly_rejects_bad_values_and_preserves_exact_source_bytes() {
    let good = json!({
        "schema_version": 1, "source_schema": "mission-bundle-v1",
        "id": Uuid::new_v4(), "title": "Title"
    });
    for (field, invalid) in [
        ("schema_version", json!(99)),
        ("schema_version", json!(0)),
        ("schema_version", json!("1")),
        ("schema_version", json!(-1)),
        ("source_schema", json!("mission-source-v99")),
        ("title", json!(" \n")),
        ("title", json!(false)),
        ("id", json!("not-a-uuid")),
        ("id", json!(Uuid::nil())),
        ("unknown", json!({"future": "must survive"})),
    ] {
        let mut value = good.clone();
        value[field] = invalid;
        let bytes = serde_json::to_vec_pretty(&value).unwrap();
        let failure = SharedEntity::from_json(&bytes).unwrap_err();
        assert_eq!(failure.source_bytes, bytes, "{field}");
        assert!(failure.diagnostic.code.starts_with("mission-shared-schema"));
    }
    for bytes in [b"{not json".as_slice(), b"{}".as_slice()] {
        assert_eq!(
            SharedEntity::from_json(bytes).unwrap_err().source_bytes,
            bytes
        );
    }
    assert!(SharedEntity::from_json(&serde_json::to_vec(&good).unwrap()).is_ok());
}

#[test]
fn typed_mission_and_native_versions_are_not_coerced() {
    let workspace = fixture();
    let accepted = mission(workspace.path());
    let mut typed = MissionLifecycle::open(workspace.path())
        .create_draft("Typed".into(), "Objective".into())
        .unwrap();
    assert_eq!(
        SharedEntity::from_typed_mission(&typed)
            .unwrap()
            .source_schema,
        SharedSourceSchema::MissionSourceV2
    );
    typed.schema_version = 1;
    assert!(SharedEntity::from_typed_mission(&typed).is_err());
    let mut unsupported = accepted;
    unsupported.bundle.manifest.schema_version = 2;
    assert!(SharedEntity::from_mission(&unsupported).is_err());
}

#[test]
fn canonical_references_resolve_legacy_and_v2_spec_owned_criteria_without_writes() {
    let workspace = fixture();
    for version in [false, true] {
        let (_, reference) = spec(workspace.path(), version);
        let before = snapshot(workspace.path());
        let criterion = reference.resolve(workspace.path()).unwrap();
        let rendered = render_criterion_entity(&reference, &criterion).unwrap();
        assert!(rendered.contains(&reference.specification_uri()));
        assert!(rendered.contains("Authoritative &lt;criterion&gt; \\| \\[text\\]"));
        assert_eq!(snapshot(workspace.path()), before);
    }
}

#[test]
fn dangling_wrong_spec_and_wrong_root_references_fail_without_fallback() {
    let workspace = fixture();
    let elsewhere = fixture();
    let (_, mut reference) = spec(elsewhere.path(), true);
    assert!(reference.resolve(workspace.path()).is_err());
    let (_, valid) = spec(workspace.path(), true);
    reference.specification_id = valid.specification_id;
    reference.criterion_id = CriterionId::new("other-criterion").unwrap();
    assert!(reference.resolve(workspace.path()).is_err());
    reference.specification_id = Uuid::new_v4();
    assert!(reference.resolve(workspace.path()).is_err());
    let (_, other_spec) = spec(workspace.path(), true);
    let resolved = valid.resolve(workspace.path()).unwrap();
    // Same criterion ID in another Spec is not the same criterion.
    assert!(render_criterion_entity(&other_spec, &resolved).is_err());
}

#[test]
fn malformed_canonical_spec_identity_version_and_duplicate_criteria_fail() {
    let workspace = fixture();
    let (original, reference) = spec(workspace.path(), true);
    let path = spec_path(workspace.path(), original.id);
    for invalid in [
        json!({"format_version": 99}),
        json!({"format_version": "2"}),
        json!({"criteria": [{"criterion_id": "schema-check"}]}),
        json!({"criteria": "not-an-array"}),
    ] {
        let mut changed = original.clone();
        for (key, value) in invalid.as_object().unwrap() {
            changed.extra.insert(key.clone(), value.clone());
        }
        write_spec(workspace.path(), &changed);
        assert!(reference.resolve(workspace.path()).is_err());
    }
    let mut changed = original.clone();
    changed.id = Uuid::new_v4();
    fs::write(&path, toml::to_string(&changed).unwrap()).unwrap();
    assert!(reference.resolve(workspace.path()).is_err());
    let mut changed = original;
    let criterion = changed.extra["criteria"][0].clone();
    changed
        .extra
        .insert("criteria".into(), json!([criterion, criterion]));
    write_spec(workspace.path(), &changed);
    assert!(reference.resolve(workspace.path()).is_err());
}
