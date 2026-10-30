use super::*;

#[test]
#[ignore = "explicitly regenerate committed review outputs through Mission APIs"]
fn regenerate_reviewable_outputs() {
    let corpus = fixture_paths();
    let authoring = corpus.join("authoring");
    let generated = corpus.join("generated");
    fs::create_dir_all(&generated).unwrap();
    for (slug, filename) in fixture_cases() {
        let workspace = tempfile::Builder::new()
            .prefix(".representative-w8-generation-")
            .tempdir_in(Path::new(env!("CARGO_MANIFEST_DIR")))
            .unwrap();
        let mission_id = generate_fixture(workspace.path(), &authoring.join(filename));
        let output = generated.join(slug);
        if output.exists() {
            fs::remove_dir_all(&output).unwrap();
        }
        copy_tree(&generated_directory(workspace.path(), mission_id), &output);
    }
}

#[test]
fn committed_corpus_is_byte_identical_to_fresh_api_generation() {
    let corpus = fixture_paths();
    for (slug, filename) in fixture_cases() {
        let workspace = tempfile::Builder::new()
            .prefix(".representative-w8-verify-")
            .tempdir_in(Path::new(env!("CARGO_MANIFEST_DIR")))
            .unwrap();
        let mission_id =
            generate_fixture(workspace.path(), &corpus.join("authoring").join(filename));
        let store = MissionStore::open(workspace.path()).with_test_event_metadata(
            Uuid::from_u128(0x0000000000004000800000000000f00d),
            1_798_000_000_000,
        );
        let accepted = store.get(mission_id).unwrap();
        assert_eq!(accepted.mission_id, mission_id);
        assert_eq!(
            store.output_contract(mission_id).unwrap().renderer,
            "mission-api/7"
        );
        store.check_generated_output(mission_id).unwrap();
        store.verify_output_history(mission_id).unwrap();
        let canonical_root = workspace.path().canonicalize().unwrap();
        for bytes in tree(&generated_directory(workspace.path(), mission_id)).values() {
            let content = String::from_utf8_lossy(bytes);
            assert!(
                !content.contains(&canonical_root.to_string_lossy().to_string()),
                "generated output leaked its temporary workspace path for {slug}"
            );
        }
        let roadmap = fs::read_to_string(
            generated_directory(workspace.path(), mission_id).join("generated/ROADMAP.md"),
        )
        .unwrap();
        assert!(roadmap.contains(
            "Owning workspace: selected workspace root (local records in .workflow-tools/)."
        ));
        assert_eq!(
            tree(&corpus.join("generated").join(slug)),
            tree(&generated_directory(workspace.path(), mission_id)),
            "complete Mission tree differs for {slug}"
        );
    }
}
