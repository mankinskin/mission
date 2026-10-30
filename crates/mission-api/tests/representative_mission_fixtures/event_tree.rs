use super::*;

#[test]
fn test_only_event_metadata_provider_reproduces_complete_event_tree() {
    let mission_id = Uuid::from_u128(0x00000000000040008000000000000001);
    let event_id = Uuid::from_u128(0x00000000000040008000000000000002);
    let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let first_root = tempfile::Builder::new()
        .prefix(".representative-w8-first-")
        .tempdir_in(fixture_root)
        .unwrap();
    let second_root = tempfile::Builder::new()
        .prefix(".representative-w8-second-")
        .tempdir_in(fixture_root)
        .unwrap();

    let first = generate_event_tree(first_root.path(), mission_id, event_id);
    let second = generate_event_tree(second_root.path(), mission_id, event_id);

    assert_eq!(first, second);
}
