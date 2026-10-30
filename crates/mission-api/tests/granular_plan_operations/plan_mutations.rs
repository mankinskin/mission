use super::*;

#[test]
fn goal_create_update_and_reorder_publish_coherent_revisions_and_total_order() {
    let f = seeded(3, &["a", "b", "c"]);
    let ticket = Uuid::from_u128(0x3300);
    let ticket_dir = f
        .root()
        .join(".workflow-tools/ticket/tickets")
        .join(ticket.to_string());
    fs::create_dir_all(&ticket_dir).unwrap();
    fs::write(
        ticket_dir.join("ticket.toml"),
        format!("id = \"{ticket}\"\ncreated_at = \"2026-10-29T00:00:00Z\"\ntitle = \"Existing Ticket\"\nstate = \"ready\"\n"),
    ).unwrap();
    assert_eq!(
        apply(
            &f,
            "g",
            2,
            PlanOperation::SetGoal {
                title: "Retitled".into(),
                objective: "New goal".into()
            }
        )
        .unwrap()
        .revision,
        3
    );
    let mut updated = waypoint("a");
    updated.prompt = "changed".into();
    updated.ownership_refs.ticket_id = Some(ticket);
    assert_eq!(
        apply(
            &f,
            "u",
            3,
            PlanOperation::UpdateWaypoint { waypoint: updated }
        )
        .unwrap()
        .revision,
        4
    );
    assert_eq!(
        apply(
            &f,
            "c",
            4,
            PlanOperation::CreateWaypoint {
                waypoint: waypoint("d"),
                after: Some("a".into())
            }
        )
        .unwrap()
        .revision,
        5
    );
    assert_eq!(source_ids(&f), ["a", "d", "b", "c"]);
    assert_eq!(
        apply(
            &f,
            "o",
            5,
            PlanOperation::ReorderWaypoints {
                order: vec!["c".into(), "a".into(), "d".into(), "b".into()]
            }
        )
        .unwrap()
        .revision,
        6
    );
    let source = f.lifecycle.get_mission_source(f.id).unwrap();
    assert_eq!(source.revision, 6);
    assert_eq!(
        (source.title.as_str(), source.objective.as_str()),
        ("Retitled", "New goal")
    );
    let chain: Vec<_> = source
        .waypoints
        .iter()
        .map(|w| (w.waypoint_id.as_str(), w.depends_on.clone()))
        .collect();
    assert_eq!(
        chain,
        [
            ("c", vec![]),
            ("a", vec!["c".to_string()]),
            ("d", vec!["a".to_string()]),
            ("b", vec!["d".to_string()])
        ]
    );
    let accepted = f.lifecycle.get(f.id).unwrap();
    assert_eq!(accepted.bundle.manifest.revision, 6);
    assert_eq!(
        accepted.bundle.manifest.execution_order,
        ["c", "a", "d", "b"]
    );
    let plan = review(&f);
    assert_eq!(plan["revision"], 6);
    assert_eq!(projection(&f), plan);
    let refs = plan["waypoints"][1]["references"].as_array().unwrap();
    assert!(
        refs.iter()
            .any(|r| r.as_str().unwrap().contains(&ticket.to_string()))
    );
}

#[test]
fn invalid_reorder_is_rejected_without_changes() {
    let f = seeded(3, &["a", "b", "c"]);
    for (n, order) in [
        ("missing", vec!["a", "b"]),
        ("duplicate", vec!["a", "a", "b", "c"]),
    ] {
        rejected(
            &f,
            n,
            2,
            PlanOperation::ReorderWaypoints {
                order: order.into_iter().map(str::to_owned).collect(),
            },
            "invalid_order",
        );
    }
}

#[test]
fn removal_of_referenced_work_needs_a_same_operation_replacement_and_keeps_results() {
    let f = seeded(3, &["a", "b", "c"]);
    rejected(
        &f,
        "rm",
        2,
        PlanOperation::RemoveWaypoint {
            waypoint_id: "b".into(),
            replacement: None,
        },
        "replacement_required",
    );
    rejected(
        &f,
        "dup",
        2,
        PlanOperation::RemoveWaypoint {
            waypoint_id: "b".into(),
            replacement: Some(waypoint("c")),
        },
        "invalid-part-reference",
    );
    let before = review(&f)["plan_digest"].clone();
    let o = apply(
        &f,
        "rm2",
        2,
        PlanOperation::RemoveWaypoint {
            waypoint_id: "b".into(),
            replacement: Some(waypoint("b2")),
        },
    )
    .unwrap();
    assert_eq!(o.revision, 3);
    assert_eq!(source_ids(&f), ["a", "b2", "c"]);
    assert_eq!(
        f.lifecycle.get_mission_source(f.id).unwrap().waypoints[2].depends_on,
        ["b2"]
    );
    assert_ne!(Value::String(o.plan_digest.clone()), before);
    apply(
        &f,
        "rm3",
        3,
        PlanOperation::RemoveWaypoint {
            waypoint_id: "c".into(),
            replacement: None,
        },
    )
    .unwrap();
    assert_eq!(source_ids(&f), ["a", "b2"]);
    let digest = review(&f)["plan_digest"].as_str().unwrap().to_owned();
    apply(
        &f,
        "ev",
        4,
        PlanOperation::RecordEvidence {
            plan_digest: digest,
            result: evidence("a"),
        },
    )
    .unwrap();
    rejected(
        &f,
        "rm4",
        5,
        PlanOperation::RemoveWaypoint {
            waypoint_id: "a".into(),
            replacement: Some(waypoint("a2")),
        },
        "retained-waypoint-required",
    );
    assert_eq!(source_ids(&f), ["a", "b2"]);
}
