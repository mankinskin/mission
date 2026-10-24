pub use serde_json::{Value, json};
pub use std::process::Command;
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Child, ChildStdin, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};
pub use tempfile::TempDir;

pub fn cli(workspace: &Path, arguments: &[String]) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_mission"))
        .arg("--workspace")
        .arg(workspace)
        .args(arguments)
        .output()
        .expect("mission CLI starts");
    let snapshot: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "CLI JSON expected ({error}); stdout: {}; stderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    assert_eq!(
        output.status.success(),
        snapshot["status"] == "ok",
        "{snapshot}"
    );
    snapshot
}

pub fn assert_ok(snapshot: &Value) {
    assert_eq!(snapshot["status"], "ok", "{snapshot}");
}

pub fn revision(snapshot: &Value) -> u64 {
    snapshot["revision"].as_u64().expect("accepted revision")
}

pub struct Mcp {
    child: Child,
    input: ChildStdin,
    responses: Receiver<Result<Value, String>>,
}

impl Mcp {
    pub fn start(workspace: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_mission-mcp"))
            .current_dir(workspace)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("mission MCP starts");
        let input = child.stdin.take().expect("MCP stdin");
        let stdout = child.stdout.take().expect("MCP stdout");
        let (sender, responses) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let item = line.map_err(|error| error.to_string()).and_then(|line| {
                    serde_json::from_str(&line).map_err(|error| error.to_string())
                });
                if sender.send(item).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            input,
            responses,
        }
    }

    pub fn request(&mut self, id: u64, method: &str, params: Value) -> Value {
        writeln!(
            self.input,
            "{}",
            json!({"jsonrpc":"2.0", "id":id, "method":method, "params":params})
        )
        .and_then(|_| self.input.flush())
        .expect("write MCP request");
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let response = self
                .responses
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .expect("bounded MCP response")
                .expect("valid MCP JSON response");
            if response.get("id") != Some(&json!(id)) {
                continue;
            }
            assert!(
                response.get("error").is_none(),
                "structured MCP error: {response}"
            );
            return response["result"].clone();
        }
    }

    pub fn initialize(&mut self) {
        self.request(
            1,
            "initialize",
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "direct-transports", "version": "1"}
            }),
        );
        writeln!(
            self.input,
            "{}",
            json!({"jsonrpc":"2.0", "method":"notifications/initialized"})
        )
        .and_then(|_| self.input.flush())
        .expect("write MCP initialized notification");
    }

    pub fn call(&mut self, id: u64, tool: &str, arguments: Value) -> Value {
        let result = self.request(
            id,
            "tools/call",
            json!({"name":tool, "arguments":arguments}),
        );
        assert_ne!(
            result["isError"],
            Value::Bool(true),
            "tool returned isError: {result}"
        );
        let text = result["content"][0]["text"]
            .as_str()
            .expect("structured tool text");
        serde_json::from_str(text).expect("Mission snapshot in tool text")
    }
}

impl Drop for Mcp {
    fn drop(&mut self) {
        let _ = self.child.kill();
        self.child.wait().expect("owned MCP child exits");
    }
}

pub fn waypoint() -> Value {
    json!({
        "waypoint_id": "install",
        "title": "Install fixture",
        "depends_on": [],
        "prompt": "Run the install fixture and retain its evidence.",
        "validation": [{"command":"operator test command", "cwd":"mission"}],
        "commit_checkpoint": {"message":"runtime evidence only"},
        "non_goals": ["Do not run external commands"]
    })
}

pub fn accepted_waypoints(snapshot: &Value) -> BTreeMap<String, Value> {
    snapshot["accepted"]["bundle"]["waypoints"]
        .as_array()
        .expect("accepted waypoints")
        .iter()
        .map(|waypoint| {
            (
                waypoint["waypoint_id"]
                    .as_str()
                    .expect("waypoint id")
                    .to_owned(),
                waypoint.clone(),
            )
        })
        .collect()
}
