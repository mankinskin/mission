//! Fixture hook subprocess; this is plumbing, not human authorization.
use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::OnceLock,
};
static HOOK: OnceLock<PathBuf> = OnceLock::new();

fn hook_binary() -> &'static Path {
    HOOK.get_or_init(|| {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../Cargo.toml");
        let output = Command::new(env!("CARGO"))
            .args([
                "build",
                "--locked",
                "--message-format=json-render-diagnostics",
                "-p",
                "session",
                "--bin",
                "session-capture-hook",
                "--features",
                "capture-hook",
                "--manifest-path",
            ])
            .arg(manifest)
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .output()
            .unwrap();
        assert!(output.status.success(), "building capture hook failed");
        String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .filter(|v| {
                v["reason"] == "compiler-artifact"
                    && v["target"]["name"] == "session-capture-hook"
                    && v["target"]["kind"]
                        .as_array()
                        .is_some_and(|k| k.iter().any(|x| x == "bin"))
            })
            .filter_map(|v| v["executable"].as_str().map(PathBuf::from))
            .last()
            .expect("cargo capture hook artifact")
    })
}
pub fn run(
    checkout: &Path,
    session_id: &str,
    mut payload: serde_json::Value,
) -> std::process::Output {
    let transcript = checkout.join("t.jsonl");
    std::fs::write(&transcript, "").unwrap();
    payload["session_id"] = session_id.into();
    payload["transcript_path"] = transcript.to_str().unwrap().into();
    let mut child = Command::new(hook_binary())
        .arg("--from-hook-stdin")
        .current_dir(checkout)
        .env("MCP_MAIN_CHECKOUT", checkout)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(payload.to_string().as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}
