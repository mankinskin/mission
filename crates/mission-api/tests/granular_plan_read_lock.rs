use mission_api::*;
use std::{
    fs,
    path::{Path, PathBuf},
};
use tempfile::{TempDir, tempdir};
use uuid::Uuid;

fn seeded() -> (TempDir, MissionLifecycle, Uuid) {
    let dir = tempdir().unwrap();
    let lifecycle = MissionLifecycle::open(dir.path());
    let mut draft = lifecycle
        .create_draft("Plan".into(), "Goal".into())
        .unwrap();
    let id = draft.mission_id;
    draft.waypoints = vec![TypedWaypoint {
        waypoint_id: "a".into(),
        title: "a title".into(),
        prompt: "a prompt".into(),
        part: PartSource {
            waypoint_id: "a".into(),
            document: Document {
                blocks: vec![Block::Paragraph {
                    inlines: vec![Inline::Text {
                        text: "a body".into(),
                    }],
                }],
            },
        },
        validation: vec![ValidationGate {
            command: "cargo test".into(),
            cwd: None,
        }],
        commit_checkpoint: CommitCheckpoint {
            message: "a checkpoint".into(),
        },
        non_goals: vec!["a non-goal".into()],
        ..Default::default()
    }];
    lifecycle.publish_draft(id, draft, 1).unwrap();
    (dir, lifecycle, id)
}
fn flock(lifecycle: &MissionLifecycle, id: Uuid) -> PathBuf {
    lifecycle
        .mission_directory(id)
        .parent()
        .unwrap()
        .join(format!(".{id}.typed.flock"))
}
fn names(dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    out.sort();
    out
}

#[test]
fn healthy_read_takes_the_existing_lock_and_writes_nothing() {
    let (_dir, lifecycle, id) = seeded();
    let root = lifecycle
        .mission_directory(id)
        .parent()
        .unwrap()
        .to_path_buf();
    let before = (names(&root), fs::read(flock(&lifecycle, id)).unwrap());
    lifecycle.review_plan(id).unwrap();
    assert_eq!(
        before,
        (names(&root), fs::read(flock(&lifecycle, id)).unwrap())
    );
}

#[test]
fn missing_lock_is_an_explicit_recovery_error_and_is_not_created() {
    let (_dir, lifecycle, id) = seeded();
    let lock = flock(&lifecycle, id);
    fs::remove_file(&lock).unwrap();
    let root = lock.parent().unwrap().to_path_buf();
    let before = names(&root);
    let error = lifecycle.review_plan(id).unwrap_err();
    assert!(
        matches!(error, MissionError::PublicationRecovery { .. }),
        "{error:?}"
    );
    assert!(!lock.exists(), "a read must not create the lock file");
    assert_eq!(names(&root), before);
}

#[cfg(unix)]
#[test]
fn symlinked_or_broken_lock_is_refused_not_followed() {
    let (dir, lifecycle, id) = seeded();
    let lock = flock(&lifecycle, id);
    fs::remove_file(&lock).unwrap();
    let target = dir.path().join("elsewhere.flock");
    for existing in [false, true] {
        if existing {
            fs::write(&target, b"").unwrap();
        }
        std::os::unix::fs::symlink(&target, &lock).unwrap();
        let error = lifecycle.review_plan(id).unwrap_err();
        assert!(
            matches!(error, MissionError::PublicationRecovery { .. }),
            "existing={existing}: {error:?}"
        );
        assert!(
            fs::symlink_metadata(&lock)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        fs::remove_file(&lock).unwrap();
    }
}

#[test]
fn absent_mission_reports_not_found_and_creates_no_lock() {
    let (_dir, lifecycle, _id) = seeded();
    let root = lifecycle
        .mission_directory(Uuid::new_v4())
        .parent()
        .unwrap()
        .to_path_buf();
    let before = names(&root);
    let error = lifecycle.review_plan(Uuid::new_v4()).unwrap_err();
    assert!(matches!(error, MissionError::NotFound(_)), "{error:?}");
    assert_eq!(names(&root), before);
}
