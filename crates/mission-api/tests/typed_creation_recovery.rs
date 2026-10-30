use mission_api::{MissionError, MissionLifecycle};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Barrier, mpsc},
    thread,
    time::{Duration, Instant},
};
use uuid::Uuid;

const TITLE: &str = "Planning Mission";
const OBJECTIVE: &str = "Intake-derived placeholder";
fn missions_root(ws: &Path) -> PathBuf {
    ws.join(".workflow-tools/mission/missions")
}
fn stage_path(ws: &Path, id: Uuid) -> PathBuf {
    missions_root(ws).join(format!(".{id}.typed.tmp"))
}
fn create(ws: &Path, id: Uuid, title: &str) -> Result<(), MissionError> {
    MissionLifecycle::open(ws)
        .create_draft_with_id(id, title.into(), OBJECTIVE.into())
        .map(|_| ())
}

fn snapshot(path: &Path) -> BTreeMap<String, Option<Vec<u8>>> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, Option<Vec<u8>>>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let key = path.strip_prefix(root).unwrap().display().to_string();
            let meta = fs::symlink_metadata(&path).unwrap();
            if meta.file_type().is_symlink() {
                out.insert(key, Some(b"<symlink>".to_vec()));
            } else if meta.is_dir() {
                out.insert(key, None);
                walk(root, &path, out);
            } else {
                out.insert(key, Some(fs::read(&path).unwrap()));
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(path, path, &mut out);
    out
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn seed(ws: &Path, id: Uuid, title: &str) -> PathBuf {
    create(ws, id, title).unwrap();
    missions_root(ws).join(id.to_string())
}
fn plant_stage(ws: &Path, id: Uuid, from: &Path) -> PathBuf {
    fs::create_dir_all(missions_root(ws)).unwrap();
    let stage = stage_path(ws, id);
    copy_tree(from, &stage);
    stage
}
fn assert_recovery_required(result: Result<(), MissionError>, id: Uuid) {
    assert!(
        matches!(&result, Err(MissionError::PublicationRecovery { mission_id, .. }) if *mission_id == id),
        "{result:?}"
    );
}
fn assert_stage_left_untouched(ws: &Path, id: Uuid, stage: &Path, title: &str) {
    let before = snapshot(stage);
    assert_recovery_required(create(ws, id, title), id);
    assert_eq!(snapshot(stage), before);
    assert!(!missions_root(ws).join(id.to_string()).exists());
}

#[test]
fn stale_flock_file_does_not_hang_creation() {
    let ws = tempfile::tempdir().unwrap();
    let id = Uuid::new_v4();
    let root = missions_root(ws.path());
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join(format!(".{id}.typed.flock")), b"crashed").unwrap();
    let (tx, rx) = mpsc::channel();
    let path = ws.path().to_path_buf();
    thread::spawn(move || {
        let _ = tx.send(create(&path, id, TITLE));
    });
    rx.recv_timeout(Duration::from_secs(20))
        .expect("stale flock must not hang")
        .unwrap();
}

#[test]
fn legacy_lock_directory_yields_a_bounded_recovery_error_without_publishing() {
    let ws = tempfile::tempdir().unwrap();
    let id = Uuid::new_v4();
    let root = missions_root(ws.path());
    fs::create_dir_all(&root).unwrap();
    let legacy = root.join(format!(".{id}.typed.lock"));
    fs::create_dir(&legacy).unwrap();
    let (tx, rx) = mpsc::channel();
    let path = ws.path().to_path_buf();
    thread::spawn(move || {
        let _ = tx.send(create(&path, id, TITLE));
    });
    let result = rx
        .recv_timeout(Duration::from_secs(20))
        .expect("legacy lock must fail boundedly");
    assert_recovery_required(result, id);
    assert!(legacy.is_dir());
    assert!(!root.join(id.to_string()).exists());
    assert!(!stage_path(ws.path(), id).exists());
}

#[test]
fn held_lock_is_waited_for_then_released() {
    let ws = tempfile::tempdir().unwrap();
    let id = Uuid::new_v4();
    let root = missions_root(ws.path());
    fs::create_dir_all(&root).unwrap();
    let holder = fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(root.join(format!(".{id}.typed.flock")))
        .unwrap();
    holder.lock().unwrap();
    let release = thread::spawn(move || {
        thread::sleep(Duration::from_millis(300));
        drop(holder);
    });
    let started = Instant::now();
    create(ws.path(), id, TITLE).unwrap();
    assert!(started.elapsed() >= Duration::from_millis(250));
    release.join().unwrap();
}

#[test]
fn concurrent_creators_yield_exactly_one_winner() {
    let ws = tempfile::tempdir().unwrap();
    let id = Uuid::new_v4();
    let barrier = Arc::new(Barrier::new(8));
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let (path, barrier) = (ws.path().to_path_buf(), barrier.clone());
            thread::spawn(move || {
                barrier.wait();
                create(&path, id, TITLE)
            })
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(
        results.iter().filter(|r| r.is_ok()).count(),
        1,
        "{results:?}"
    );
    for result in results.iter().filter(|r| r.is_err()) {
        assert!(
            matches!(
                result,
                Err(MissionError::StaleRevision {
                    expected: 0,
                    actual: 1
                })
            ),
            "{result:?}"
        );
    }
    assert!(!stage_path(ws.path(), id).exists());
}

#[test]
fn complete_exact_stage_without_intent_is_adopted_for_the_same_uuid() {
    let (reference, ws) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let id = Uuid::new_v4();
    let built = seed(reference.path(), id, TITLE);
    let stage = plant_stage(ws.path(), id, &built);
    create(ws.path(), id, TITLE).unwrap();
    assert!(!stage.exists());
    assert_eq!(
        snapshot(&missions_root(ws.path()).join(id.to_string())),
        snapshot(&built)
    );
}

#[test]
fn stage_for_different_content_is_never_adopted_or_deleted() {
    let (reference, ws) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let id = Uuid::new_v4();
    let built = seed(reference.path(), id, "Some other title");
    let stage = plant_stage(ws.path(), id, &built);
    assert_stage_left_untouched(ws.path(), id, &stage, TITLE);
}

#[test]
fn stage_for_a_different_mission_identity_is_never_adopted_or_deleted() {
    let (reference, ws) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let (id, other) = (Uuid::new_v4(), Uuid::new_v4());
    let built = seed(reference.path(), other, TITLE);
    let stage = plant_stage(ws.path(), id, &built);
    assert_stage_left_untouched(ws.path(), id, &stage, TITLE);
}

#[test]
fn stage_with_extra_or_corrupt_content_is_never_adopted_or_deleted() {
    let reference = tempfile::tempdir().unwrap();
    let id = Uuid::new_v4();
    let built = seed(reference.path(), id, TITLE);
    let extra = tempfile::tempdir().unwrap();
    let stage = plant_stage(extra.path(), id, &built);
    fs::write(stage.join("notes.txt"), b"foreign").unwrap();
    assert_stage_left_untouched(extra.path(), id, &stage, TITLE);
    let corrupt = tempfile::tempdir().unwrap();
    let stage = plant_stage(corrupt.path(), id, &built);
    let mut bytes = fs::read(stage.join("accepted.json")).unwrap();
    let middle = bytes.len() / 2;
    bytes[middle] ^= 0x01;
    fs::write(stage.join("accepted.json"), bytes).unwrap();
    assert_stage_left_untouched(corrupt.path(), id, &stage, TITLE);
}

#[cfg(unix)]
#[test]
fn stage_containing_a_symlink_is_never_adopted_or_deleted() {
    let reference = tempfile::tempdir().unwrap();
    let ws = tempfile::tempdir().unwrap();
    let id = Uuid::new_v4();
    let built = seed(reference.path(), id, TITLE);
    let stage = plant_stage(ws.path(), id, &built);
    std::os::unix::fs::symlink(ws.path(), stage.join("escape")).unwrap();
    assert_stage_left_untouched(ws.path(), id, &stage, TITLE);
}

#[cfg(unix)]
#[test]
fn dangling_stage_symlink_is_preserved_and_refused() {
    let ws = tempfile::tempdir().unwrap();
    let id = Uuid::new_v4();
    fs::create_dir_all(missions_root(ws.path())).unwrap();
    let target = ws.path().join("missing-outside");
    std::os::unix::fs::symlink(&target, stage_path(ws.path(), id)).unwrap();
    assert_recovery_required(create(ws.path(), id, TITLE), id);
    assert_eq!(fs::read_link(stage_path(ws.path(), id)).unwrap(), target);
    assert!(!missions_root(ws.path()).join(id.to_string()).exists());
}

#[test]
fn stage_next_to_an_existing_mission_still_requires_manual_recovery() {
    let ws = tempfile::tempdir().unwrap();
    let id = Uuid::new_v4();
    let built = seed(ws.path(), id, TITLE);
    let stage = plant_stage(ws.path(), id, &built);
    let before = snapshot(&stage);
    let api = MissionLifecycle::open(ws.path());
    let mut draft = api.get_mission_source(id).unwrap();
    draft.title = "Edited".into();
    assert_recovery_required(api.save_draft(id, draft, 1).map(|_| ()), id);
    assert_eq!(snapshot(&stage), before);
}
