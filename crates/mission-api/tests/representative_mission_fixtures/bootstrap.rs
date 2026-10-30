use super::*;

#[test]
#[ignore = "one-time creation of canonical API authoring inputs"]
fn bootstrap_authoring_inputs() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/representative-missions/authoring");
    fs::create_dir_all(&root).unwrap();
    for (name, value) in [
        (
            "documentation-workflow.native-v2.json",
            serde_json::to_vec_pretty(&native_input(DOC_IDENTITY, DOC_SEEDS, false, None, None, 3))
                .unwrap(),
        ),
        (
            "mission-integration.typed-v3.json",
            serde_json::to_vec_pretty(&typed_draft(MISSION_IDENTITY)).unwrap(),
        ),
        (
            "ticket-reliability.native-v2.json",
            serde_json::to_vec_pretty(&native_input(
                TICKET_IDENTITY,
                TICKET_SEEDS,
                true,
                Some(15),
                Some(16),
                14,
            ))
            .unwrap(),
        ),
    ] {
        fs::write(root.join(name), value).unwrap();
    }
}
