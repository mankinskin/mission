use super::TypedLock;
use crate::MissionError;
use std::time::{Duration, Instant};
use uuid::Uuid;

#[test]
fn stale_flock_file_is_recovered_automatically() {
    let dir = tempfile::tempdir().unwrap();
    let id = Uuid::new_v4();
    std::fs::write(
        dir.path().join(format!(".{id}.typed.flock")),
        b"crashed writer",
    )
    .unwrap();
    let started = Instant::now();
    let _lock = TypedLock::acquire_for(dir.path(), id, Duration::from_secs(5)).unwrap();
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn legacy_lock_directory_fails_boundedly_and_is_preserved() {
    let dir = tempfile::tempdir().unwrap();
    let id = Uuid::new_v4();
    let legacy = dir.path().join(format!(".{id}.typed.lock"));
    std::fs::create_dir(&legacy).unwrap();
    let started = Instant::now();
    let error = match TypedLock::acquire_for(dir.path(), id, Duration::from_millis(300)) {
        Ok(_) => panic!("unproven legacy ownership must not be bypassed"),
        Err(error) => error,
    };
    assert!(started.elapsed() < Duration::from_secs(5));
    assert!(
        matches!(&error, MissionError::PublicationRecovery { mission_id, .. } if *mission_id == id),
        "{error:?}"
    );
    assert!(legacy.is_dir());
    std::fs::remove_dir(&legacy).unwrap();
    let _lock = TypedLock::acquire_for(dir.path(), id, Duration::from_secs(5)).unwrap();
}

#[cfg(unix)]
#[test]
fn a_symlinked_lock_file_is_refused_and_its_target_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let id = Uuid::new_v4();
    let target = outside.path().join("victim");
    std::fs::write(&target, b"keep").unwrap();
    std::os::unix::fs::symlink(&target, dir.path().join(format!(".{id}.typed.flock"))).unwrap();
    let result = TypedLock::acquire_for(dir.path(), id, Duration::from_millis(300));
    assert!(matches!(
        result,
        Err(MissionError::PublicationRecovery { .. })
    ));
    assert_eq!(std::fs::read(&target).unwrap(), b"keep");
}

#[test]
fn legacy_lock_directory_released_by_its_owner_within_grace_is_waited_for() {
    let dir = tempfile::tempdir().unwrap();
    let id = Uuid::new_v4();
    let legacy = dir.path().join(format!(".{id}.typed.lock"));
    std::fs::create_dir(&legacy).unwrap();
    let remover = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(200));
        std::fs::remove_dir(legacy).unwrap();
    });
    let _lock = TypedLock::acquire_for(dir.path(), id, Duration::from_secs(5)).unwrap();
    remover.join().unwrap();
}

#[test]
fn held_lock_times_out_with_a_bounded_diagnostic() {
    let dir = tempfile::tempdir().unwrap();
    let id = Uuid::new_v4();
    let _held = TypedLock::acquire_for(dir.path(), id, Duration::from_secs(5)).unwrap();
    let started = Instant::now();
    let error = match TypedLock::acquire_for(dir.path(), id, Duration::from_millis(150)) {
        Ok(_) => panic!("held lock must not be acquired twice"),
        Err(error) => error,
    };
    assert!(started.elapsed() >= Duration::from_millis(150));
    assert!(started.elapsed() < Duration::from_secs(5));
    assert!(
        matches!(&error, MissionError::Validation(_, d) if d[0].code == "typed-lock-timeout"),
        "{error:?}"
    );
}

#[test]
fn released_lock_is_reacquirable_and_locks_are_per_mission() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
    let timeout = Duration::from_secs(5);
    let first = TypedLock::acquire_for(dir.path(), a, timeout).unwrap();
    let _other = TypedLock::acquire_for(dir.path(), b, timeout).unwrap();
    drop(first);
    let _again = TypedLock::acquire_for(dir.path(), a, timeout).unwrap();
}
