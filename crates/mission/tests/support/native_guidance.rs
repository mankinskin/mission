use markdown_links::{DestinationShape, classify_destination, extract, resolve_local};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
};
#[path = "native_guidance_links.rs"]
mod native_guidance_links;

pub const SOURCES: [&str; 12] = [
    ".agents/prompts/refine-ingest.prompt.md",
    ".agents/prompts/execute-ingest.prompt.md",
    ".agents/prompts/execute-roadmap.prompt.md",
    "workflow-tools/.agents/instructions/workflow/prompt-ingestion.instructions.md",
    "workflow-tools/.agents/instructions/workflow/roadmap-authoring.instructions.md",
    "workflow-tools/.agents/instructions/workflow/roadmap-execution.instructions.md",
    ".agents/instructions/workflow/mission-tooling.instructions.md",
    "workflow-tools/ticket/.agents/instructions/ticket/workflow.instructions.md",
    "workflow-tools/spec/.agents/instructions/spec/spec-system.instructions.md",
    "workflow-tools/mission/docs/provider-evidence.md",
    "workflow-tools/mission/docs/intent-authority.md",
    "workflow-tools/mission/docs/README.md",
];

pub fn source_root() -> PathBuf {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(4)
        .expect("Mission crate belongs to the meta source root")
        .to_path_buf();
    assert!(root.join(SOURCES[0]).is_file());
    root
}

pub struct ScratchDirectory {
    path: PathBuf,
}

impl ScratchDirectory {
    pub fn new() -> Self {
        let parent = source_root().join("workflow-tools/target/mission-native-routing-tests");
        fs::create_dir_all(&parent).unwrap();
        let path = parent.join(uuid::Uuid::new_v4().to_string());
        fs::create_dir(&path).unwrap();
        Self { path }
    }

    pub fn path(&self) -> &Path {
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
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let output = Command::new("cargo")
            .args([
                "metadata",
                "--no-deps",
                "--format-version",
                "1",
                "--manifest-path",
            ])
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
        .arg(source_root().join(
            "workflow-tools/mission/crates/mission/tests/fixtures/mission-native-guidance.toml",
        ))
        .args(["--select", "mission-native-route", "--target"])
        .arg(target)
        .arg("--json")
        .output()
        .unwrap();
    let mut stream = serde_json::Deserializer::from_slice(&output.stdout).into_iter::<Value>();
    let plan = stream.next().unwrap().unwrap();
    let diagnostics = plan["diagnostics"].as_array().unwrap();
    assert!(output.status.success(), "{diagnostics:?}");
    let blocking: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| {
            !matches!(
                diagnostic["code"].as_str(),
                Some("cycle" | "unlocatable_rewrite")
            )
        })
        .collect();
    assert!(
        blocking.is_empty(),
        "{} blocking diagnostics: {:?}",
        blocking.len(),
        &blocking[..blocking.len().min(8)]
    );
    (
        plan,
        String::from_utf8(output.stdout[stream.byte_offset()..].to_vec()).unwrap(),
    )
}

fn rewritten(source: &str, rewrites: &[Value]) -> String {
    let destinations: BTreeMap<_, _> = rewrites
        .iter()
        .map(|rewrite| {
            (
                rewrite["old_link"].as_str().unwrap(),
                rewrite["new_link"].as_str().unwrap(),
            )
        })
        .collect();
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
    let base = relative
        .rsplit_once('/')
        .map_or("", |(directory, _)| directory);
    for link in extract(text) {
        if let DestinationShape::Local(path) = classify_destination(&link.destination)
            && (path.ends_with(".md") || path.ends_with(".toml"))
        {
            let resolved = resolve_local(base, &path).unwrap();
            assert!(
                target.join(&resolved).is_file(),
                "{relative}: unresolved installed link {}",
                link.destination
            );
        }
    }
}

pub fn assert_routing_contract(read: impl Fn(&str) -> String) {
    native_guidance_links::assert_contract(&read, &SOURCES);
    let ingestion = read(SOURCES[3]);
    for marker in [
        "## Native new-run planning",
        "mission_intake_create",
        "mission_workspace_get",
        "mission.native-planning version=1",
        "native_capability_unavailable",
        "approval_authority_unavailable",
        "UserPromptSubmit",
        "decision_refs",
        "mission_plan_record_review",
        "cold_review_id",
        "mission_plan_prepare_review",
        "mission_plan_approve",
        "mission_plan_replan",
        "source_path",
        "first complete waypoint",
        "fresh challenge",
        "five ordered transcript passes",
        "both current-revision",
        "distinct formal review",
        "goal_coverage_findings",
        "terminal_outcome_finding",
        "scope_findings",
        "mission-intent-confirm",
    ] {
        assert!(
            ingestion.contains(marker),
            "missing native contract {marker}"
        );
    }
    for source in &SOURCES[..3] {
        let prompt = read(source);
        assert!(
            prompt.contains("Native new-run planning")
                || prompt.contains("Native Mission execution")
        );
        assert!(prompt.contains("classic"));
        assert!(
            !prompt.contains("20-tool discovery"),
            "stale discovery contract"
        );
        assert!(
            !prompt.contains("original 19"),
            "discovery belongs to the instruction owner"
        );
        if *source != SOURCES[2] {
            assert!(prompt.contains("source-to-outcome-fidelity"));
        }
    }
    let authoring = read(SOURCES[4])
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    for marker in [
        "read-only projections",
        "## Source-to-outcome fidelity",
        "exact immutable source",
        "observable final behavior",
        "owning waypoint",
        "executable acceptance",
        "requester-receipted exclusion",
        "both the cold walkthrough and the distinct formal review",
        "If every waypoint succeeds, is the requester's original intended outcome actually delivered?",
        "design before implementation",
        "documentation-only",
        "working search",
        "search design document",
        "completeness",
        "compactness",
        "approachability",
        "single sitting",
        "preconditions",
        "postconditions",
        "read/write roots",
        "known dead ends",
    ] {
        assert!(
            authoring.contains(marker),
            "missing fidelity contract {marker}"
        );
    }
    for example in [
        "| search | confirmed input span: \"Implement working search\" | working search returns matching records | implement-search, verify-search | integration check exercises the real search path | none |",
        "| design | confirmed input span: \"Write only the search design\" | reviewed search design document | write-design | deterministic schema/link/example checks | none: artifact is the requested outcome |",
        "Substituting a search design document is a negative example",
        "Mocks satisfy acceptance only when the original confirmed policy permits them.",
    ] {
        assert!(
            authoring.contains(example),
            "missing outcome/policy example {example}"
        );
    }
    let execution = read(SOURCES[5])
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    for marker in [
        "## Native Mission execution",
        "effective execution eligibility",
        "## Mission-backed execution",
        "one justified diagnostic rerun",
        "block or escalate",
        "cargo test --locked -p mission --features cli,mcp,test-evidence-provider --test mission_native_routing -j1",
        "provider-evidence.md",
    ] {
        assert!(
            execution.contains(marker),
            "missing execution contract {marker}"
        );
    }
    let tooling = read(SOURCES[6]).to_lowercase();
    for marker in [
        "immutable",
        "amendments",
        "success authority",
        "provider-evidence.md",
    ] {
        assert!(
            tooling.contains(marker),
            "missing Mission authority {marker}"
        );
    }
    assert!(read(SOURCES[7]).contains("Tickets own How"));
    assert!(read(SOURCES[8]).contains("Specs own What"));
    let intent = read(SOURCES[10]);
    for marker in [
        "prepare_baseline",
        "confirm_baseline",
        "prepare_amendment",
        "append_amendment",
        "reference: {session_id, challenge_id}",
        "working_behavior",
        "artifact",
        "decision",
        "research_review",
        "intent_action",
        "prior_chain_sha256",
        "exclude",
        "replace",
    ] {
        assert!(intent.contains(marker), "missing intent operation {marker}");
    }
    let provider = read(SOURCES[9]);
    for marker in [
        "no production adapter",
        "capability_unavailable",
        "no local command fallback",
        "not kernel/process isolation",
        "at most one diagnostic rerun",
        "schema identity",
        "exact producer/consumer",
        "SHA-256",
        "mock_evidence_prohibited",
        "test-evidence-provider",
    ] {
        assert!(
            provider.contains(marker),
            "missing provider boundary {marker}"
        );
    }
}

pub fn installation_roundtrip(initialized: bool, nested: bool) {
    let target = ScratchDirectory::new();
    let sentinel = target.path().join("classic/ROADMAP.md");
    fs::create_dir(target.path().join("classic")).unwrap();
    fs::write(&sentinel, "Existing active classic provenance\n").unwrap();
    if initialized {
        fs::create_dir(target.path().join("workflow-tools")).unwrap();
        fs::write(
            target.path().join("workflow-tools/.git"),
            "gitdir: ../.git/modules/workflow-tools\n",
        )
        .unwrap();
    }
    if nested {
        for owner in ["ticket", "spec"] {
            let directory = target.path().join("workflow-tools").join(owner);
            fs::create_dir_all(&directory).unwrap();
            fs::write(directory.join(".git"), "gitdir: fixture-owned\n").unwrap();
        }
    }
    #[cfg(unix)]
    for owner in if initialized {
        vec!["", "workflow-tools"]
    } else {
        vec![""]
    } {
        let directory = target.path().join(owner).join(".claude");
        fs::create_dir_all(&directory).unwrap();
        std::os::unix::fs::symlink("../.agents/prompts", directory.join("commands")).unwrap();
    }
    let (plan, _) = guidance("plan", target.path());
    assert!(
        !target.path().join(".agents").exists(),
        "plan must be read-only"
    );
    let artifacts = plan["artifacts"].as_array().unwrap();
    assert!(
        artifacts.len() > SOURCES.len(),
        "required transitive closure is installed"
    );
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
        let artifact = artifacts
            .iter()
            .find(|artifact| artifact["id"] == source)
            .unwrap();
        if source.starts_with("workflow-tools/mission/docs/") {
            assert_eq!(artifact["final_path"], format!(".agents/{source}"));
            continue;
        }
        let expected = if let Some(relative) = source
            .strip_prefix("workflow-tools/ticket/")
            .or_else(|| source.strip_prefix("workflow-tools/spec/"))
            && !nested
        {
            relative
        } else if source.starts_with("workflow-tools/") && !initialized {
            source.strip_prefix("workflow-tools/").unwrap()
        } else {
            source
        };
        assert_eq!(artifact["final_path"], expected);
    }
    let (installed_plan, first) = guidance("install", target.path());
    assert_eq!(installed_plan, plan);
    assert!(
        first.contains(&format!(
            "installed {} artifact(s), 0 unchanged",
            artifacts.len()
        )),
        "{first}"
    );
    for artifact in artifacts {
        let id = artifact["id"].as_str().unwrap();
        let path = artifact["final_path"].as_str().unwrap();
        let installed = fs::read_to_string(target.path().join(path)).unwrap();
        let expected = rewritten(&originals[id], artifact["rewrites"].as_array().unwrap());
        assert_eq!(installed, expected, "{id}: source/rewrite parity");
        assert_guidance_links(target.path(), path, &installed);
        assert_eq!(
            hashes[id],
            format!(
                "{:x}",
                Sha256::digest(fs::read(source_root().join(id)).unwrap())
            ),
            "{id}: source changed"
        );
    }
    assert_routing_contract(|id| {
        let artifact = artifacts
            .iter()
            .find(|artifact| artifact["id"] == id)
            .unwrap();
        fs::read_to_string(target.path().join(artifact["final_path"].as_str().unwrap())).unwrap()
    });
    let (_, second) = guidance("install", target.path());
    assert!(
        second.contains(&format!(
            "installed 0 artifact(s), {} unchanged",
            artifacts.len()
        )),
        "{second}"
    );
    assert_eq!(
        fs::read_to_string(sentinel).unwrap(),
        "Existing active classic provenance\n"
    );
    #[cfg(unix)]
    for owner in if initialized {
        vec!["", "workflow-tools"]
    } else {
        vec![""]
    } {
        let commands = target.path().join(owner).join(".claude/commands");
        assert_eq!(
            fs::read_link(&commands).unwrap(),
            Path::new("../.agents/prompts")
        );
        if !owner.is_empty() {
            // This profile selects root prompts, not the independent product prompt corpus.
            continue;
        }
        for entry in fs::read_dir(&commands).unwrap() {
            let entry = entry.unwrap();
            if entry.path().is_file() {
                assert_eq!(
                    fs::read(entry.path()).unwrap(),
                    fs::read(
                        target
                            .path()
                            .join(owner)
                            .join(".agents/prompts")
                            .join(entry.file_name())
                    )
                    .unwrap()
                );
            }
        }
    }
}
