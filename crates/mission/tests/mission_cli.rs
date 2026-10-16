use std::process::Command;

fn mission() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mission"))
}

#[test]
fn top_level_help_succeeds() {
    let output = mission().arg("--help").output().unwrap();

    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("Usage:"));
}

#[test]
fn validate_preview_help_succeeds() {
    let output = mission()
        .args(["validate-preview", "--help"])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("Usage:"));
}

#[test]
fn invalid_arguments_fail() {
    let output = mission().arg("not-a-command").output().unwrap();

    assert!(!output.status.success());
}
