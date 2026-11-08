use mission_api::MissionStore;

#[test]
fn init_upgrades_store_gitignore_without_changing_durable_records() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join(".workflow-tools/mission");
    std::fs::create_dir_all(&root).unwrap();
    let original = "# custom\ncustom-output/";
    std::fs::write(root.join(".gitignore"), original).unwrap();

    MissionStore::init(dir.path()).unwrap();
    let first = std::fs::read_to_string(root.join(".gitignore")).unwrap();
    assert!(first.starts_with(original));
    for entry in [
        ".*.lock",
        "*.flock",
        ".*-lock",
        "*.tmp",
        ".*.tmp-*",
        ".tmp-*",
        "move-locks/",
        "scan_fingerprints.json",
    ] {
        assert!(first.lines().any(|line| line == entry), "missing {entry}");
    }
    MissionStore::init(dir.path()).unwrap();
    assert_eq!(
        std::fs::read_to_string(root.join(".gitignore")).unwrap(),
        first
    );
}

#[test]
fn open_does_not_create_store_or_gitignore() {
    let dir = tempfile::tempdir().unwrap();
    let _store = MissionStore::open(dir.path());
    assert!(!dir.path().join(".workflow-tools").exists());
}
