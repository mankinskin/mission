#[path = "support/direct_transports.rs"]
mod support;
use support::*;

fn create_root_fixture(workspace: &std::path::Path, title: &str) -> String {
    let created = cli(
        workspace,
        &[
            "create-mission".into(),
            title.into(),
            "Workspace selector fixture.".into(),
            json!([waypoint()]).to_string(),
        ],
    );
    assert_ok(&created);
    created["mission_id"]
        .as_str()
        .expect("mission id")
        .to_owned()
}

fn cli_get_id(
    cwd: &std::path::Path,
    explicit: Option<&std::path::Path>,
    configured: Option<&std::ffi::OsStr>,
    mission_id: &str,
) -> Value {
    cli_launch(
        cwd,
        explicit,
        configured,
        &["get".into(), mission_id.into()],
    )
}

#[path = "direct_transports/lifecycle_cli.rs"]
mod lifecycle_cli;
#[path = "direct_transports/transport_parity.rs"]
mod transport_parity;
#[path = "direct_transports/workspace_resolution.rs"]
mod workspace_resolution;
