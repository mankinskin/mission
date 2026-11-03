#[path = "support/direct_transports.rs"]
mod direct_transports;

use direct_transports::Mcp;
use markdown_links::{DestinationShape, classify_destination, extract, resolve_local};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::OnceLock,
};

const SESSION: &str = "11111111-1111-4111-8111-111111111111";
const NATIVE_TOOLS: [&str; 20] = [
    "mission_intake_create",
    "mission_workspace_get",
    "mission_plan_get",
    "mission_plan_update",
    "mission_waypoint_create",
    "mission_waypoint_update",
    "mission_waypoint_remove",
    "mission_waypoint_reorder",
    "mission_waypoint_complete",
    "mission_plan_prepare_review",
    "mission_plan_approve",
    "mission_plan_replan",
    "mission_pause",
    "mission_resume",
    "mission_handoff",
    "mission_cancel",
    "mission_complete",
    "mission_review_results_add_waypoints",
    "mission_reopen_most_recent_completed",
    "mission_plan_record_review",
];
const SOURCES: [&str; 6] = [
    ".agents/prompts/refine-ingest.prompt.md",
    ".agents/prompts/execute-ingest.prompt.md",
    ".agents/prompts/execute-roadmap.prompt.md",
    "workflow-tools/.agents/instructions/workflow/prompt-ingestion.instructions.md",
    "workflow-tools/.agents/instructions/workflow/roadmap-authoring.instructions.md",
    "workflow-tools/.agents/instructions/workflow/roadmap-execution.instructions.md",
];

struct ScratchDirectory {
    path: PathBuf,
}

impl ScratchDirectory {
    fn new() -> Self {
        let parent = source_root().join("workflow-tools/target/mission-native-routing-tests");
        fs::create_dir_all(&parent).unwrap();
        let path = parent.join(uuid::Uuid::new_v4().to_string());
        fs::create_dir(&path).unwrap();
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for ScratchDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
        if let Some(parent) = self.path.parent() {
            let _ = fs::remove_dir(parent);
        }
    }
}

fn installer() -> &'static PathBuf {
    static BINARY: OnceLock<PathBuf> = OnceLock::new();
    BINARY.get_or_init(|| {
        let manifest = source_root().join("workflow-tools/install/install-ctl/Cargo.toml");
        let output = Command::new("cargo")
            .args(["build", "--locked", "--manifest-path"])
            .arg(&manifest)
            .args(["-j", "1"])
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let output = Command::new("cargo")
            .args(["metadata", "--no-deps", "--format-version", "1", "--manifest-path"])
            .arg(manifest)
            .output()
            .unwrap();
        assert!(output.status.success());
        let metadata: Value = serde_json::from_slice(&output.stdout).unwrap();
        PathBuf::from(metadata["target_directory"].as_str().unwrap())
            .join("debug")
            .join(format!("install-ctl{}", std::env::consts::EXE_SUFFIX))
    })
}

fn guidance(operation: &str, target: &Path) -> (Value, String) {
    let output = Command::new(installer())
        .args(["guidance", operation, "--source"])
        .arg(source_root())
        .arg("--profile")
        .arg(source_root().join("workflow-tools/mission/crates/mission/tests/fixtures/mission-native-guidance.toml"))
        .args(["--select", "mission-native-route", "--target"])
        .arg(target)
        .arg("--json")
        .output()
        .unwrap();
    let mut stream = serde_json::Deserializer::from_slice(&output.stdout).into_iter::<Value>();
    let plan = stream.next().unwrap().unwrap();
    let diagnostics = plan["diagnostics"].as_array().unwrap();
    assert!(output.status.success(), "{} diagnostics: {:?}", diagnostics.len(),
            &diagnostics[..diagnostics.len().min(8)]);
    let blocking: Vec<_> = diagnostics.iter()
        .filter(|diagnostic| !matches!(
            diagnostic["code"].as_str(),
            Some("cycle" | "missing_dependency" | "unlocatable_rewrite")
        ))
        .collect();
    assert!(blocking.is_empty(), "{} blocking diagnostics: {:?}",
            blocking.len(), &blocking[..blocking.len().min(8)]);
    let missing: Vec<_> = diagnostics.iter()
        .filter(|diagnostic| diagnostic["code"] == "missing_dependency")
        .collect();
    assert!(missing.is_empty(), "{} missing dependencies: {:?}", missing.len(),
            &missing[..missing.len().min(10)]);
    (plan, String::from_utf8(output.stdout[stream.byte_offset()..].to_vec()).unwrap())
}

fn rewritten(source: &str, rewrites: &[Value]) -> String {
    let destinations: BTreeMap<_, _> = rewrites.iter().map(|rewrite| (
        rewrite["old_link"].as_str().unwrap(), rewrite["new_link"].as_str().unwrap()
    )).collect();
    let mut edits = BTreeMap::new();
    for link in extract(source) {
        if let Some(replacement) = destinations.get(link.destination.as_str())
            && let Some(span) = link.span
        {
            edits.insert((span.start, span.end), *replacement);
        }
    }
    let mut expected = source.to_owned();
    for ((start, end), replacement) in edits.into_iter().rev() {
        expected.replace_range(start..end, replacement);
    }
    expected
}

fn assert_guidance_links(target: &Path, relative: &str, text: &str) {
    let base = relative.rsplit_once('/').map_or("", |(directory, _)| directory);
    for link in extract(text) {
        if let DestinationShape::Local(path) = classify_destination(&link.destination)
            && (path.ends_with(".md") || path.ends_with(".toml"))
        {
            let resolved = resolve_local(base, &path).unwrap();
            assert!(target.join(&resolved).is_file(),
                    "{relative}: unresolved installed link {}", link.destination);
        }
    }
}

fn assert_routing_contract(read: impl Fn(&str) -> String) {
    let ingestion = read(SOURCES[3]);
    for marker in [
        "## Native new-run planning", "mission_intake_create", "mission_workspace_get",
        "mission.native-planning version=1", "native_capability_unavailable",
        "approval_authority_unavailable", "UserPromptSubmit", "decision_refs",
        "mission_plan_record_review", "cold_review_id", "mission_plan_prepare_review",
        "mission_plan_approve", "mission_plan_replan",
        "source_path", "first complete waypoint", "fresh challenge",
        "five ordered transcript passes", "both current-revision", "distinct formal review",
    ] {
        assert!(ingestion.contains(marker), "missing native contract {marker}");
    }
    for source in &SOURCES[..3] {
        let prompt = read(source);
        assert!(prompt.contains("Native new-run planning") || prompt.contains("Native Mission execution"));
        assert!(prompt.contains("classic"));
    }
    assert!(read(SOURCES[4]).contains("read-only projections"));
    let execution = read(SOURCES[5]);
    assert!(execution.contains("## Native Mission execution"));
    assert!(execution.contains("effective execution eligibility"));
    assert!(execution.contains("## Mission-backed execution"));
}

fn installation_roundtrip(initialized: bool) {
    let target = ScratchDirectory::new();
    let sentinel = target.path().join("classic/ROADMAP.md");
    fs::create_dir(target.path().join("classic")).unwrap();
    fs::write(&sentinel, "Existing active classic provenance\n").unwrap();
    if initialized {
        fs::create_dir(target.path().join("workflow-tools")).unwrap();
        fs::write(target.path().join("workflow-tools/.git"), "gitdir: ../.git/modules/workflow-tools\n").unwrap();
    }
    #[cfg(unix)]
    for owner in if initialized { vec!["", "workflow-tools"] } else { vec![""] } {
        let directory = target.path().join(owner).join(".claude");
        fs::create_dir_all(&directory).unwrap();
        std::os::unix::fs::symlink("../.agents/prompts", directory.join("commands")).unwrap();
    }
    let (plan, _) = guidance("plan", target.path());
    assert!(!target.path().join(".agents").exists(), "plan must be read-only");
    let artifacts = plan["artifacts"].as_array().unwrap();
    assert!(artifacts.len() > SOURCES.len(), "required transitive closure is installed");
    let mut paths = BTreeSet::new();
    let mut originals = BTreeMap::new();
    let mut hashes = BTreeMap::new();
    for artifact in artifacts {
        let id = artifact["id"].as_str().unwrap();
        let path = artifact["final_path"].as_str().unwrap();
        assert!(paths.insert(path), "duplicate destination {path}");
        let source = fs::read_to_string(source_root().join(id)).unwrap();
        hashes.insert(id, format!("{:x}", Sha256::digest(source.as_bytes())));
        originals.insert(id, source);
    }
    for source in SOURCES {
        let artifact = artifacts.iter().find(|artifact| artifact["id"] == source).unwrap();
        let expected = if source.starts_with("workflow-tools/") && !initialized {
            source.strip_prefix("workflow-tools/").unwrap()
        } else { source };
        assert_eq!(artifact["final_path"], expected);
    }
    let (installed_plan, first) = guidance("install", target.path());
    assert_eq!(installed_plan, plan);
    assert!(first.contains(&format!("installed {} artifact(s), 0 unchanged", artifacts.len())), "{first}");
    for artifact in artifacts {
        let id = artifact["id"].as_str().unwrap();
        let path = artifact["final_path"].as_str().unwrap();
        let installed = fs::read_to_string(target.path().join(path)).unwrap();
        let expected = rewritten(&originals[id], artifact["rewrites"].as_array().unwrap());
        assert_eq!(format!("{:x}", Sha256::digest(installed.as_bytes())),
                   format!("{:x}", Sha256::digest(expected.as_bytes())), "{id}: source/rewrite parity");
        assert_guidance_links(target.path(), path, &installed);
        assert_eq!(hashes[id], format!("{:x}", Sha256::digest(fs::read(source_root().join(id)).unwrap())),
                   "{id}: source changed");
    }
    assert_routing_contract(|id| {
        let artifact = artifacts.iter().find(|artifact| artifact["id"] == id).unwrap();
        fs::read_to_string(target.path().join(artifact["final_path"].as_str().unwrap())).unwrap()
    });
    let (_, second) = guidance("install", target.path());
    assert!(second.contains(&format!("installed 0 artifact(s), {} unchanged", artifacts.len())), "{second}");
    assert_eq!(fs::read_to_string(sentinel).unwrap(), "Existing active classic provenance\n");
    #[cfg(unix)]
    {
        assert_eq!(fs::read_link(target.path().join(".claude/commands")).unwrap(), Path::new("../.agents/prompts"));
        assert_eq!(fs::read(target.path().join(".claude/commands/refine-ingest.prompt.md")).unwrap(),
                   fs::read(target.path().join(".agents/prompts/refine-ingest.prompt.md")).unwrap());
    }
}

#[test]
fn native_guidance_installs_rewritten_closure_in_consumer_root_idempotently() {
    installation_roundtrip(false);
}

#[test]
fn native_guidance_prefers_initialized_product_submodule_without_changing_classic_sources() {
    installation_roundtrip(true);
}

#[test]
fn canonical_sources_and_existing_aliases_follow_same_native_contract() {
    assert_routing_contract(|path| fs::read_to_string(source_root().join(path)).unwrap());
    let installer_readme =
        fs::read_to_string(source_root().join("workflow-tools/install/README.md")).unwrap();
    assert!(installer_readme.contains("mission_plan_record_review"));
    assert!(installer_readme.contains("simulate a real requester receipt"));
    #[cfg(unix)]
    for owner in ["", "workflow-tools"] {
        assert_eq!(fs::read_link(source_root().join(owner).join(".claude/commands")).unwrap(),
                   Path::new("../.agents/prompts"));
    }
}

fn route(arguments: &[&str]) -> Output {
    Command::new("python3")
        .arg(source_root().join("workflow-tools/tools/roadmap-route.py"))
        .args(arguments)
        .output().unwrap()
}

fn mcp_error_envelope(mcp: &mut Mcp, request_id: u64, tool: &str, arguments: Value) -> Value {
    let response = mcp.request(
        request_id,
        "tools/call",
        json!({"name":tool, "arguments":arguments}),
    );
    assert_eq!(response["isError"], true, "{response}");
    serde_json::from_str(response["content"][0]["text"].as_str().unwrap()).unwrap()
}

#[test]
fn classic_and_explicit_migration_routes_preserve_their_existing_output() {
    for (mode, source, action) in [
        ("classic", "dossier", "execute-dossier"),
        ("mission-backed", "mission", "migrate-dossier"),
    ] {
        let output = route(&["--mode", mode, "--dossier", "classic/ROADMAP.md"]);
        assert!(output.status.success());
        assert_eq!(serde_json::from_slice::<Value>(&output.stdout).unwrap(),
            json!({"mode":mode,"source":source,"action":action,"dossier":"classic/ROADMAP.md"}));
    }
}

fn source_root() -> PathBuf {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(4)
        .expect("Mission crate belongs to the meta source root")
        .to_path_buf();
    assert!(root.join(".agents/prompts/refine-ingest.prompt.md").is_file());
    root
}

#[test]
fn native_new_run_without_discovery_fails_visibly_without_classic_fallback() {
    let output = route(&["--mode", "native-new-run"]);
    assert!(!output.status.success());
    let error: Value = serde_json::from_slice(&output.stderr)
        .unwrap_or_else(|_| panic!("{}", String::from_utf8_lossy(&output.stderr)));
    assert_eq!(error["code"], "native_capability_unavailable");
    assert_eq!(error["fallback"], json!(null));
}

#[test]
fn native_discovery_and_fresh_intake_stop_before_requester_approval() {
    use session_api::{SessionRuntimeInitRequest, SessionStoreConfig};
    let temporary = ScratchDirectory::new();
    let root = temporary.path().join("workspace");
    fs::create_dir(&root).unwrap();
    SessionStoreConfig::new(root.join(".workflow-tools/session"))
        .init_runtime_context(SessionRuntimeInitRequest {
            session_id: Some(SESSION.into()), predecessor_run_id: None, force_new_run: false,
        }).unwrap();
    let mut mcp = Mcp::start(&root);
    mcp.initialize();
    let listed = mcp.request(2, "tools/list", json!({}));
    let tools = listed["tools"].as_array().unwrap();
    for name in NATIVE_TOOLS {
        assert_eq!(
            tools.iter().filter(|tool| tool["name"] == name).count(),
            1,
            "tools/list must expose exactly one {name}"
        );
    }
    let discovery = temporary.path().join("tools.json");
    fs::write(&discovery, serde_json::to_vec(&listed).unwrap()).unwrap();
    let arguments = ["--mode","native-new-run","--workspace-root",root.to_str().unwrap(),
                     "--session-id",SESSION,"--tools-list",discovery.to_str().unwrap()];
    let selected = route(&arguments);
    let review_schema = listed["tools"].as_array().unwrap().iter()
        .find(|tool| tool["name"] == "mission_plan_record_review").unwrap()["inputSchema"].clone();
    assert!(selected.status.success(), "{}; review schema: {review_schema}",
            String::from_utf8_lossy(&selected.stderr));
    let selected: Value = serde_json::from_slice(&selected.stdout).unwrap();
    assert_eq!(selected["action"], "plan-native");
    assert_eq!(selected["source"], "session");
    assert_eq!(selected["executable"], false);
    assert!(!root.join(".workflow-tools/mission").exists(), "selector must not create a Mission");

    let mut missing_review = listed.clone();
    missing_review["tools"].as_array_mut().unwrap()
        .retain(|tool| tool["name"] != "mission_plan_record_review");
    fs::write(&discovery, serde_json::to_vec(&missing_review).unwrap()).unwrap();
    let old_capability = route(&arguments);
    assert!(!old_capability.status.success(),
            "the original 19 tools alone must not select native planning");
    let error: Value = serde_json::from_slice(&old_capability.stderr).unwrap();
    assert_eq!(error["code"], "native_capability_unavailable");
    assert_eq!(error["fallback"], Value::Null);
    fs::write(&discovery, serde_json::to_vec(&listed).unwrap()).unwrap();

    let intake = mcp.call(3, "mission_intake_create", json!({
        "workspace_root":root,"session_id":SESSION,"request_id":"w7-source",
        "input":"Keep immutable native source\n",
    }));
    assert_eq!(intake["code"], "ok", "{intake}");
    let workspace = &intake["workspace"];
    assert_eq!(fs::read_to_string(workspace["input_path"].as_str().unwrap()).unwrap(),
               "Keep immutable native source\n");
    assert!(!root.join("transcripts").exists());
    let mission_dir = Path::new(workspace["mission_dir"].as_str().unwrap());
    let source_path = Path::new(workspace["source_path"].as_str().unwrap());
    assert!(mission_dir.is_dir());
    assert!(source_path.is_file());
    assert_eq!(source_path.file_name().unwrap(), "source.json");
    let located = mcp.call(4, "mission_workspace_get", json!({
        "workspace_root":root,"session_id":SESSION,"request_id":"w7-source",
    }));
    assert_eq!(located["workspace"], intake["workspace"]);
    assert_eq!(intake["result"]["executable"], false);
    let id = intake["mission_id"].as_str().unwrap();
    let waypoint = mission::TypedWaypoint {
        waypoint_id: "w1".into(),
        title: "Native route".into(),
        prompt: "Verify native routing".into(),
        part: mission::PartSource {
            waypoint_id: "w1".into(),
            document: mission::Document {
                blocks: vec![mission::Block::Paragraph {
                    inlines: vec![mission::Inline::Text { text: "Native routing outcome".into() }],
                }],
            },
        },
        validation: vec![mission::ValidationGate { command: "true".into(), cwd: None }],
        commit_checkpoint: mission::CommitCheckpoint { message: "Native checkpoint".into() },
        non_goals: vec!["Unrelated work".into()],
        ..Default::default()
    };
    let created = mcp.call(5, "mission_waypoint_create", json!({
        "workspace_root":root,"mission_id":id,"request_id":"w7-waypoint",
        "expected_revision":intake["revision"],"waypoint":waypoint
    }));
    assert_eq!(created["code"], "ok", "{created}");
    let read = mcp.call(6, "mission_plan_get", json!({"workspace_root":root,"mission_id":id}));
    assert_eq!(read["result"]["grants"], json!([]));
    let review_args = json!({
        "workspace_root":root,"mission_id":id,"expected_revision":created["revision"],
        "plan_digest":read["result"]["plan"]["plan_digest"],
        "evidence_identity":read["result"]["plan"]["evidence_identity"],
        "kind":"cold_walkthrough","verdict":"pass","reviewer":"cold-reviewer",
        "request_id":"w7-cold-review"
    });
    let cold = mcp.call(7, "mission_plan_record_review", review_args.clone());
    assert_eq!(cold["code"], "ok", "{cold}");
    let mut formal_args = review_args.clone();
    formal_args["kind"] = json!("formal");
    formal_args["reviewer"] = json!("formal-reviewer");
    formal_args["request_id"] = json!("w7-formal-review");
    formal_args["cold_review_id"] = cold["result"]["review_id"].clone();
    let formal = mcp.call(8, "mission_plan_record_review", formal_args);
    assert_eq!(formal["code"], "ok", "{formal}");
    assert_eq!(cold["revision"], created["revision"]);
    assert_eq!(formal["revision"], created["revision"]);
    assert_ne!(cold["result"]["reviewer"], formal["result"]["reviewer"]);
    assert_eq!(formal["result"]["cold_review_id"], cold["result"]["review_id"]);

    let reviewed = mcp.call(9, "mission_plan_get", json!({"workspace_root":root,"mission_id":id}));
    assert_eq!(reviewed["result"]["reviews"], json!([cold["result"], formal["result"]]));
    assert_eq!(reviewed["result"]["grants"], json!([]));
    assert_eq!(reviewed["result"]["decision_refs"], json!([]));

    let challenge = mcp.call(10, "mission_plan_prepare_review", json!({
        "workspace_root":root,"mission_id":id,"request_id":"w7-review",
        "expected_revision":created["revision"],"session_id":SESSION,
    }));
    assert_eq!(challenge["code"], "ok", "{challenge}");
    assert!(challenge["result"]["host_prompt"].as_str().unwrap()
        .starts_with("mission-decision approve "));

    let denied = mcp_error_envelope(&mut mcp, 11, "mission_plan_approve", json!({
        "workspace_root":root,"mission_id":id,"request_id":challenge["result"]["challenge_id"],
        "expected_revision":created["revision"],
        "decision":{"session_id":SESSION,"challenge_id":challenge["result"]["challenge_id"],
                    "decision_id":"absent","source_event_id":"absent"}
    }));
    assert_eq!(denied["code"], "approval_authority_unavailable", "{denied}");
    let fake_authority = mcp_error_envelope(&mut mcp, 12, "mission_plan_approve", json!({
        "workspace_root":root,"mission_id":id,"request_id":challenge["result"]["challenge_id"],
        "expected_revision":created["revision"],
        "decision":{"session_id":SESSION,"challenge_id":challenge["result"]["challenge_id"],
                    "decision_id":"agent-approval","source_event_id":"invented",
                    "actor":"agent","approved":true}
    }));
    assert_ne!(fake_authority["code"], "ok", "{fake_authority}");
    let after = mcp.call(13, "mission_plan_get", json!({"workspace_root":root,"mission_id":id}));
    assert_eq!(after["result"]["grants"], json!([]));
    for mutate in [
        "missing",
        "version",
        "generic",
        "schema",
        "review_schema",
        "review_required",
        "review_optional",
        "review_kind",
        "review_ref",
        "malformed",
    ] {
        let mut changed = listed.clone();
        let tools = changed["tools"].as_array_mut().unwrap();
        match mutate {
            "missing" => { tools.retain(|tool| tool["name"] != "mission_intake_create"); }
            "generic" => { tools.retain(|tool| tool["name"] == "mission_operation"); }
            "version" => {
                tools.iter_mut().find(|tool| tool["name"]=="mission_intake_create").unwrap()["description"] =
                    json!("capability=mission.native-planning version=2");
            }
            "schema" => {
                tools.iter_mut().find(|tool| tool["name"]=="mission_intake_create").unwrap()["inputSchema"] = json!({});
            }
            "review_schema" => {
                tools.iter_mut().find(|tool| tool["name"]=="mission_plan_record_review").unwrap()
                    ["inputSchema"]["additionalProperties"] = json!(true);
            }
            "review_required" => {
                let required = tools.iter_mut()
                    .find(|tool| tool["name"]=="mission_plan_record_review").unwrap()
                    ["inputSchema"]["required"].as_array_mut().unwrap();
                required.retain(|field| field != "reviewer");
            }
            "review_optional" => {
                tools.iter_mut().find(|tool| tool["name"]=="mission_plan_record_review").unwrap()
                    ["inputSchema"]["required"].as_array_mut().unwrap().push(json!("session_root"));
            }
            "review_kind" => {
                tools.iter_mut().find(|tool| tool["name"]=="mission_plan_record_review").unwrap()
                    ["inputSchema"]["$defs"]["ReviewKind"]["enum"] = json!(["cold_walkthrough"]);
            }
            "review_ref" => {
                tools.iter_mut().find(|tool| tool["name"]=="mission_plan_record_review").unwrap()
                    ["inputSchema"]["properties"]["kind"]["enum"] = json!(["cold_walkthrough"]);
            }
            "malformed" => { changed = json!([]); }
            _ => unreachable!(),
        }
        fs::write(&discovery, serde_json::to_vec(&changed).unwrap()).unwrap();
        let output = route(&arguments);
        assert!(!output.status.success(), "{mutate} discovery must fail closed: {}",
                String::from_utf8_lossy(&output.stdout));
        let error: Value = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(error["code"], "native_capability_unavailable", "{mutate}: {error}");
        assert_eq!(error["fallback"], Value::Null);
    }
}
