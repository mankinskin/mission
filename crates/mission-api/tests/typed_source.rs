use mission_api::{
    Block, CommitCheckpoint, Document, Inline, MissionLifecycle, PartSource, TypedWaypoint,
    ValidationGate,
};
use std::fs;
use tempfile::tempdir;

fn waypoint() -> TypedWaypoint {
    TypedWaypoint {
        waypoint_id: "save".into(),
        title: "Save".into(),
        prompt: "Keep this distinct".into(),
        part: PartSource {
            waypoint_id: "save".into(),
            document: Document {
                blocks: vec![Block::Paragraph {
                    inlines: vec![Inline::Text {
                        text: "Persist safely.".into(),
                    }],
                }],
            },
        },
        validation: vec![ValidationGate {
            command: "cargo test".into(),
            cwd: Some("/tmp".into()),
        }],
        commit_checkpoint: CommitCheckpoint {
            message: "checkpoint".into(),
        },
        ..Default::default()
    }
}

    #[test]
    fn legacy_and_typed_writes_share_cas_and_preserve_independent_parts() {
        let directory = tempdir().unwrap();
        let lifecycle = MissionLifecycle::open(directory.path());
        let created = lifecycle.create_draft("Typed".into(), "objective".into()).unwrap();
        let mut draft = created.clone();
        draft.waypoints.push(waypoint());
        let published = lifecycle.publish_draft(created.mission_id, draft, 1).unwrap();
        let root = directory.path().join(".workflow-tools/mission/missions")
            .join(created.mission_id.to_string());
        let history = fs::read(root.join("history/revision-2.json")).unwrap();
        let updated = lifecycle.edit_objective(created.mission_id, "legacy edit".into(), 2).unwrap();
        assert_eq!(updated.bundle.manifest.revision, 3);
        let source = lifecycle.get_mission_source(created.mission_id).unwrap();
        assert_eq!(source.revision, 3);
        assert_eq!(source.objective, "legacy edit");
        assert_eq!(source.waypoints[0].part, published.waypoints[0].part);
        assert!(lifecycle.save_draft(created.mission_id, source.clone(), 2).is_err());
        let saved = lifecycle.save_draft(created.mission_id, source, 3).unwrap();
        assert_eq!(lifecycle.get(created.mission_id).unwrap().bundle.manifest.revision, saved.revision);
        assert_eq!(fs::read(root.join("history/revision-2.json")).unwrap(), history);
    }

#[test]
fn drafts_save_incomplete_then_publish_with_distinct_part_and_serial_edges() {
    let directory = tempdir().unwrap();
    let lifecycle = MissionLifecycle::open(directory.path());
    let created = lifecycle
        .create_draft("Typed".into(), "objective".into())
        .unwrap();
    assert_eq!(created.revision, 1);
    assert!(created.waypoints.is_empty());

    let mut draft = created.clone();
    draft.waypoints.push(waypoint());
    let saved = lifecycle.save_draft(created.mission_id, draft, 1).unwrap();
    assert_eq!(saved.revision, 2);
    assert_eq!(
        lifecycle.get_mission_source(created.mission_id).unwrap(),
        saved
    );
    assert!(
        lifecycle
            .render_mission(created.mission_id)
            .unwrap()
            .contains("Persist safely.")
    );

    let published = lifecycle
        .publish_draft(created.mission_id, saved, 2)
        .unwrap();
    assert_eq!(published.waypoints[0].depends_on, Vec::<String>::new());
    assert!(
        directory
            .path()
            .join(".workflow-tools/mission/missions")
            .join(created.mission_id.to_string())
            .join("parts/save.json")
            .is_file()
    );
    let source = std::fs::read_to_string(
        directory
            .path()
            .join(".workflow-tools/mission/missions")
            .join(created.mission_id.to_string())
            .join("source.json"),
    )
    .unwrap();
    assert!(source.contains("\"part_file\"") && !source.contains("\"document\""));
}

#[test]
fn typed_nodes_round_trip_and_rejected_writes_preserve_the_source() {
    let directory = tempdir().unwrap();
    let lifecycle = MissionLifecycle::open(directory.path());
    let created = lifecycle
        .create_draft("Typed".into(), "objective".into())
        .unwrap();
    let mut draft = created.clone();
    let mut save = waypoint();
    save.part.document.blocks = vec![
        Block::Heading {
            level: 2,
            inlines: vec![Inline::Strong {
                inlines: vec![Inline::Text {
                    text: "Outcome".into(),
                }],
            }],
        },
        Block::BulletList {
            items: vec![vec![Block::Paragraph {
                inlines: vec![Inline::Emphasis {
                    inlines: vec![Inline::Text {
                        text: "bullet".into(),
                    }],
                }],
            }]],
        },
        Block::OrderedList {
            items: vec![vec![Block::Paragraph {
                inlines: vec![Inline::Code {
                    text: "ordered".into(),
                }],
            }]],
        },
        Block::Blockquote {
            blocks: vec![Block::Paragraph {
                inlines: vec![
                    Inline::Link {
                        href: "https://example.test".into(),
                        inlines: vec![Inline::Text {
                            text: "link".into(),
                        }],
                    },
                    Inline::HardBreak,
                ],
            }],
        },
        Block::CodeFence {
            language: "rust".into(),
            text: "fn main() {}".into(),
        },
        Block::Table {
            header: vec![vec![Inline::Text {
                text: "head".into(),
            }]],
            rows: vec![vec![vec![Inline::Text {
                text: "cell".into(),
            }]]],
        },
        Block::ThematicBreak,
    ];
    draft.waypoints.push(save);
    let saved = lifecycle.save_draft(created.mission_id, draft, 1).unwrap();
    assert_eq!(
        lifecycle.get_mission_source(created.mission_id).unwrap(),
        saved
    );

    let mut unsafe_id = saved.clone();
    unsafe_id.waypoints[0].waypoint_id = "../escape".into();
    assert!(
        lifecycle
            .save_draft(created.mission_id, unsafe_id, 2)
            .is_err()
    );
    assert!(
        lifecycle
            .save_draft(created.mission_id, saved.clone(), 1)
            .is_err()
    );
    assert_eq!(
        lifecycle.get_mission_source(created.mission_id).unwrap(),
        saved
    );

    let mut incomplete = saved.clone();
    incomplete.waypoints[0].validation.clear();
    assert!(
        lifecycle
            .publish_draft(created.mission_id, incomplete, 2)
            .is_err()
    );
    assert_eq!(
        lifecycle.get_mission_source(created.mission_id).unwrap(),
        saved
    );
}

#[test]
fn draft_save_removes_stale_projection_and_rejects_bad_dependencies() {
    let directory = tempdir().unwrap();
    let lifecycle = MissionLifecycle::open(directory.path());
    let created = lifecycle
        .create_draft("Typed".into(), "objective".into())
        .unwrap();
    let mut draft = created.clone();
    draft.waypoints.push(waypoint());
    let published = lifecycle
        .publish_draft(created.mission_id, draft, 1)
        .unwrap();
    let root = directory
        .path()
        .join(".workflow-tools/mission/missions")
        .join(created.mission_id.to_string());
    assert!(root.join("mission.md").is_file());

    let mut changed = published.clone();
    changed.title = "Unpublished".into();
    let saved = lifecycle
        .save_draft(created.mission_id, changed, 2)
        .unwrap();
    assert_eq!(saved.revision, 3);
    assert!(
        !root.join("mission.md").exists(),
        "a draft must not look published"
    );

    for dependency in ["missing", "save"] {
        let mut invalid = saved.clone();
        invalid.waypoints[0].depends_on = vec![dependency.into()];
        assert!(
            lifecycle
                .save_draft(created.mission_id, invalid, 3)
                .is_err()
        );
    }
    let mut duplicate = saved.clone();
    duplicate.waypoints[0].depends_on = vec!["save".into(), "save".into()];
    assert!(
        lifecycle
            .save_draft(created.mission_id, duplicate, 3)
            .is_err()
    );
}


#[cfg(unix)]
#[test]
fn typed_source_rejects_a_symlinked_missions_ancestor() {
    use std::os::unix::fs::symlink;

    let directory = tempdir().unwrap();
    let lifecycle = MissionLifecycle::open(directory.path());
    let created = lifecycle
        .create_draft("Typed".into(), "objective".into())
        .unwrap();
    let mut draft = created.clone();
    draft.waypoints.push(waypoint());
    let saved = lifecycle.save_draft(created.mission_id, draft, 1).unwrap();

    let mission_root = directory
        .path()
        .join(".workflow-tools/mission/missions");
    let outside = directory.path().join("outside-missions");
    fs::rename(&mission_root, &outside).unwrap();
    symlink(&outside, &mission_root).unwrap();

    assert!(
        outside.join(created.mission_id.to_string()).join("source.json").is_file(),
        "the persisted source artifact was moved outside the managed root"
    );
    assert!(
        lifecycle.get_mission_source(created.mission_id).is_err(),
        "a symlinked managed ancestor must be rejected rather than read"
    );
    assert_eq!(saved.revision, 2);
}

#[cfg(unix)]
#[test]
fn typed_source_rejects_symlinked_managed_artifacts() {
    use std::os::unix::fs::symlink;

    for artifact in ["mission", "source", "parts", "part"] {
        let directory = tempdir().unwrap();
        let lifecycle = MissionLifecycle::open(directory.path());
        let created = lifecycle
            .create_draft("Typed".into(), "objective".into())
            .unwrap();
        let mut draft = created.clone();
        draft.waypoints.push(waypoint());
        let saved = lifecycle.save_draft(created.mission_id, draft, 1).unwrap();
        let mission_dir = directory
            .path()
            .join(".workflow-tools/mission/missions")
            .join(created.mission_id.to_string());
        assert_eq!(lifecycle.get_mission_source(created.mission_id).unwrap(), saved);

        let target = match artifact {
            "mission" => mission_dir.clone(),
            "source" => mission_dir.join("source.json"),
            "parts" => mission_dir.join("parts"),
            "part" => mission_dir.join("parts/save.json"),
            _ => unreachable!(),
        };
        let outside = directory.path().join(format!("outside-{artifact}"));
        fs::rename(&target, &outside).unwrap();
        symlink(&outside, &target).unwrap();

        assert!(
            lifecycle.get_mission_source(created.mission_id).is_err(),
            "a symlinked {artifact} managed artifact must be rejected"
        );
    }
}

#[test]
fn concurrent_typed_writers_have_one_accept_and_one_stale_revision() {
    use mission_api::MissionError;
    use std::sync::{Arc, Barrier};

    let directory = tempdir().unwrap();
    let lifecycle = MissionLifecycle::open(directory.path());
    let created = lifecycle
        .create_draft("Typed".into(), "objective".into())
        .unwrap();
    let mut draft = created.clone();
    draft.waypoints.push(waypoint());
    let workspace = Arc::new(directory.path().to_path_buf());
    let barrier = Arc::new(Barrier::new(2));
    let mut writers = Vec::new();

    for _ in 0..2 {
        let workspace = Arc::clone(&workspace);
        let barrier = Arc::clone(&barrier);
        let draft = draft.clone();
        let mission_id = created.mission_id;
        writers.push(std::thread::spawn(move || {
            barrier.wait();
            MissionLifecycle::open(&workspace).save_draft(mission_id, draft, 1)
        }));
    }
    let results: Vec<_> = writers
        .into_iter()
        .map(|writer| writer.join().unwrap())
        .collect();

    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|result| {
                matches!(
                    result,
                    Err(MissionError::StaleRevision {
                        expected: 1,
                        actual: 2
                    })
                )
            })
            .count(),
        1
    );
    let accepted = MissionLifecycle::open(&workspace)
        .get_mission_source(created.mission_id)
        .unwrap();
    let mission_dir = workspace
        .join(".workflow-tools/mission/missions")
        .join(created.mission_id.to_string());
    assert_eq!(accepted.revision, 2);
    assert!(mission_dir.join("source.json").is_file());
    assert!(mission_dir.join("history/revision-2.json").is_file());
    assert!(mission_dir.join("parts/save.json").is_file());
    assert!(!mission_dir.join("history/revision-3.json").exists());
}
