use mission_api::{
    Block, CommitCheckpoint, Document, Inline, MissionLifecycle, PartSource, TypedDraft,
    TypedWaypoint, ValidationGate,
};
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
