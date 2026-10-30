use mission_api::{
    CommitCheckpoint, DirectWaypoint, MissionBundle, MissionLifecycle, MissionStore, Scope,
    ValidationGate,
};
use std::{fs, path::Path, sync::mpsc, time::Duration};
use uuid::Uuid;
fn ticket(root: &Path) -> Uuid {
    let id = Uuid::new_v4();
    let path = root
        .join(".workflow-tools/ticket/tickets")
        .join(id.to_string());
    fs::create_dir_all(&path).unwrap();
    fs::write(path.join("ticket.toml"), format!(
        "id = \"{id}\"\ncreated_at = \"2026-10-29T00:00:00Z\"\ntitle = \"Existing Ticket\"\nstate = \"ready\"\n"
    )).unwrap();
    id
}
fn fixture(root: &Path) -> (MissionLifecycle, Uuid, Uuid, MissionBundle) {
    let lifecycle = MissionLifecycle::open(root);
    let accepted = lifecycle
        .create(
            "Link".into(),
            "objective".into(),
            vec![DirectWaypoint {
                waypoint_id: "first".into(),
                title: "first title".into(),
                depends_on: vec![],
                prompt: "first prompt".into(),
                validation: vec![ValidationGate {
                    command: "true".into(),
                    cwd: None,
                }],
                commit_checkpoint: CommitCheckpoint {
                    message: "first checkpoint".into(),
                },
                non_goals: vec![],
            }],
        )
        .unwrap();
    let tid = ticket(root);
    let mut bundle = accepted.bundle;
    let reference = format!("ce://default/ticket/{tid}");
    bundle.waypoints[0].ticket_ref = Some(reference.clone());
    bundle.waypoints[0].scope = Scope::TicketBacked {
        ticket_ref: reference,
    };
    bundle.waypoints[0].state = None;
    bundle.manifest.revision = 2;
    (lifecycle, accepted.mission_id, tid, bundle)
}
fn publish(
    root: &Path,
    id: Uuid,
    bundle: MissionBundle,
) -> (mpsc::Receiver<()>, mpsc::Receiver<Result<(), String>>) {
    let (rt, rr) = mpsc::channel();
    let (dt, dr) = mpsc::channel();
    let root = root.to_path_buf();
    std::thread::spawn(move || {
        rt.send(()).unwrap();
        let result = MissionStore::open(&root)
            .import(id, bundle, 1)
            .map(|_| ())
            .map_err(|e| e.to_string());
        let _ = dt.send(result);
    });
    (rr, dr)
}
#[test]
fn binding_publication_waits_for_ticket_link_lock() {
    let temp = tempfile::tempdir().unwrap();
    let (lifecycle, id, tid, bundle) = fixture(temp.path());
    let held = lifecycle.lock_ticket_links(tid).unwrap();
    let (ready, done) = publish(temp.path(), id, bundle);
    ready.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(done.recv_timeout(Duration::from_millis(500)).is_err());
    assert!(
        lifecycle.get(id).unwrap().bundle.waypoints[0]
            .ticket_ref
            .is_none()
    );
    drop(held);
    done.recv_timeout(Duration::from_secs(60)).unwrap().unwrap();
    assert!(
        lifecycle.get(id).unwrap().bundle.waypoints[0]
            .ticket_ref
            .is_some()
    );
}
#[test]
fn unrelated_ticket_lock_does_not_block_publication() {
    let temp = tempfile::tempdir().unwrap();
    let (lifecycle, id, _, bundle) = fixture(temp.path());
    let _held = lifecycle.lock_ticket_links(Uuid::new_v4()).unwrap();
    let (ready, done) = publish(temp.path(), id, bundle);
    ready.recv_timeout(Duration::from_secs(10)).unwrap();
    done.recv_timeout(Duration::from_secs(30)).unwrap().unwrap();
}
