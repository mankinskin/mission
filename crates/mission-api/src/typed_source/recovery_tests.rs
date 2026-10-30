use super::{CRASH_AFTER_INTENT, CRASH_BEFORE_PROMOTE};
use crate::{MissionError, MissionLifecycle};
use std::{
    cell::Cell,
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    thread::LocalKey,
};
use uuid::Uuid;

const TITLE: &str = "Planning Mission";
const OBJECTIVE: &str = "Intake-derived placeholder";

fn root(ws: &Path) -> PathBuf {
    ws.join(".workflow-tools/mission/missions")
}
fn stage(ws: &Path, id: Uuid) -> PathBuf {
    root(ws).join(format!(".{id}.typed.tmp"))
}
fn intent(ws: &Path, id: Uuid) -> PathBuf {
    root(ws).join(format!(".{id}.typed.intent"))
}
fn mission(ws: &Path, id: Uuid) -> PathBuf {
    root(ws).join(id.to_string())
}
fn create(ws: &Path, id: Uuid, title: &str) -> Result<(), MissionError> {
    MissionLifecycle::open(ws)
        .create_draft_with_id(id, title.into(), OBJECTIVE.into())
        .map(|_| ())
}

fn snapshot(path: &Path) -> BTreeMap<String, Option<Vec<u8>>> {
    fn walk(base: &Path, dir: &Path, out: &mut BTreeMap<String, Option<Vec<u8>>>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let key = path.strip_prefix(base).unwrap().display().to_string();
            let meta = fs::symlink_metadata(&path).unwrap();
            if meta.is_dir() {
                out.insert(key, None);
                walk(base, &path, out);
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

fn reference(id: Uuid, title: &str) -> (tempfile::TempDir, PathBuf) {
    let ws = tempfile::tempdir().unwrap();
    create(ws.path(), id, title).unwrap();
    let built = mission(ws.path(), id);
    (ws, built)
}

fn crash_create(ws: &Path, id: Uuid, title: &str, hook: &'static LocalKey<Cell<bool>>) {
    hook.with(|flag| flag.set(true));
    let result = create(ws, id, title);
    hook.with(|flag| flag.set(false));
    assert!(matches!(result, Err(MissionError::Io(_))), "{result:?}");
}

fn plant_partial(ws: &Path, id: Uuid, built: &Path) -> PathBuf {
    let stage = stage(ws, id);
    copy_tree(built, &stage);
    let accepted = fs::read(stage.join("accepted.json")).unwrap();
    fs::write(stage.join("accepted.json"), &accepted[..accepted.len() / 2]).unwrap();
    fs::remove_file(stage.join("output-contract.json")).unwrap();
    fs::remove_dir_all(stage.join("history/states")).unwrap();
    stage
}

fn assert_refused_and_untouched(ws: &Path, id: Uuid, stage: &Path) {
    let before = snapshot(stage);
    let result = create(ws, id, TITLE);
    assert!(
        matches!(&result, Err(MissionError::PublicationRecovery { mission_id, .. }) if *mission_id == id),
        "{result:?}"
    );
    assert_eq!(snapshot(stage), before);
    assert!(!mission(ws, id).exists());
}

#[test]
fn normal_creation_leaves_no_intent_behind() {
    let ws = tempfile::tempdir().unwrap();
    let id = Uuid::new_v4();
    create(ws.path(), id, TITLE).unwrap();
    assert!(!intent(ws.path(), id).exists());
    assert!(!stage(ws.path(), id).exists());
}

#[test]
fn crash_before_promote_resumes_the_same_uuid_from_the_complete_stage() {
    let id = Uuid::new_v4();
    let (_reference, built) = reference(id, TITLE);
    let ws = tempfile::tempdir().unwrap();
    crash_create(ws.path(), id, TITLE, &CRASH_BEFORE_PROMOTE);
    assert!(stage(ws.path(), id).is_dir() && intent(ws.path(), id).is_file());
    assert!(!mission(ws.path(), id).exists());
    create(ws.path(), id, TITLE).unwrap();
    assert_eq!(snapshot(&mission(ws.path(), id)), snapshot(&built));
    assert!(!stage(ws.path(), id).exists() && !intent(ws.path(), id).exists());
}

#[test]
fn crash_after_intent_with_an_own_partial_stage_is_rebuilt() {
    let id = Uuid::new_v4();
    let (_reference, built) = reference(id, TITLE);
    let ws = tempfile::tempdir().unwrap();
    crash_create(ws.path(), id, TITLE, &CRASH_AFTER_INTENT);
    assert!(intent(ws.path(), id).is_file() && !stage(ws.path(), id).exists());
    let planted = plant_partial(ws.path(), id, &built);
    create(ws.path(), id, TITLE).unwrap();
    assert!(!planted.exists());
    assert_eq!(snapshot(&mission(ws.path(), id)), snapshot(&built));
}

#[test]
fn crash_after_intent_with_an_empty_stage_is_rebuilt() {
    let id = Uuid::new_v4();
    let (_reference, built) = reference(id, TITLE);
    let ws = tempfile::tempdir().unwrap();
    crash_create(ws.path(), id, TITLE, &CRASH_AFTER_INTENT);
    fs::create_dir(stage(ws.path(), id)).unwrap();
    create(ws.path(), id, TITLE).unwrap();
    assert_eq!(snapshot(&mission(ws.path(), id)), snapshot(&built));
}

#[test]
fn a_prefix_without_a_create_intent_is_not_ownership_proof() {
    let id = Uuid::new_v4();
    let (_reference, built) = reference(id, TITLE);
    let empty = tempfile::tempdir().unwrap();
    fs::create_dir_all(root(empty.path())).unwrap();
    let stage_dir = stage(empty.path(), id);
    fs::create_dir(&stage_dir).unwrap();
    assert_refused_and_untouched(empty.path(), id, &stage_dir);
    let partial = tempfile::tempdir().unwrap();
    fs::create_dir_all(root(partial.path())).unwrap();
    let planted = plant_partial(partial.path(), id, &built);
    assert!(!intent(partial.path(), id).exists());
    assert_refused_and_untouched(partial.path(), id, &planted);
}

#[test]
fn an_intent_for_a_different_request_does_not_authorize_reconstruction() {
    let id = Uuid::new_v4();
    let (_reference, built) = reference(id, TITLE);
    let ws = tempfile::tempdir().unwrap();
    crash_create(ws.path(), id, "A different request", &CRASH_AFTER_INTENT);
    let planted = plant_partial(ws.path(), id, &built);
    assert_refused_and_untouched(ws.path(), id, &planted);
}

#[test]
fn an_intent_does_not_authorize_foreign_stage_content() {
    let id = Uuid::new_v4();
    let (_reference, built) = reference(id, TITLE);
    let ws = tempfile::tempdir().unwrap();
    crash_create(ws.path(), id, TITLE, &CRASH_AFTER_INTENT);
    let planted = plant_partial(ws.path(), id, &built);
    fs::write(planted.join("notes.txt"), b"foreign").unwrap();
    assert_refused_and_untouched(ws.path(), id, &planted);
    let corrupt = tempfile::tempdir().unwrap();
    crash_create(corrupt.path(), id, TITLE, &CRASH_AFTER_INTENT);
    let planted = plant_partial(corrupt.path(), id, &built);
    let mut bytes = fs::read(planted.join("accepted.json")).unwrap();
    bytes[0] ^= 0x01;
    fs::write(planted.join("accepted.json"), bytes).unwrap();
    assert_refused_and_untouched(corrupt.path(), id, &planted);
}

#[test]
fn a_durable_intent_binds_the_uuid_to_its_request_even_without_a_stage() {
    let id = Uuid::new_v4();
    let ws = tempfile::tempdir().unwrap();
    crash_create(ws.path(), id, "First request", &CRASH_AFTER_INTENT);
    let before = fs::read(intent(ws.path(), id)).unwrap();
    let result = create(ws.path(), id, "Second request");
    assert!(
        matches!(&result, Err(MissionError::PublicationRecovery { mission_id, .. }) if *mission_id == id),
        "{result:?}"
    );
    assert_eq!(fs::read(intent(ws.path(), id)).unwrap(), before);
    assert!(!mission(ws.path(), id).exists() && !stage(ws.path(), id).exists());
    create(ws.path(), id, "First request").unwrap();
    let source = MissionLifecycle::open(ws.path())
        .get_mission_source(id)
        .unwrap();
    assert_eq!(source.title, "First request");
    assert!(!intent(ws.path(), id).exists());
}

#[test]
fn a_foreign_or_corrupt_intent_is_preserved_and_blocks_creation() {
    let id = Uuid::new_v4();
    let ws = tempfile::tempdir().unwrap();
    fs::create_dir_all(root(ws.path())).unwrap();
    fs::write(intent(ws.path(), id), b"not an intent").unwrap();
    let result = create(ws.path(), id, TITLE);
    assert!(
        matches!(&result, Err(MissionError::PublicationRecovery { mission_id, .. }) if *mission_id == id),
        "{result:?}"
    );
    assert_eq!(fs::read(intent(ws.path(), id)).unwrap(), b"not an intent");
    assert!(!mission(ws.path(), id).exists() && !stage(ws.path(), id).exists());
}
