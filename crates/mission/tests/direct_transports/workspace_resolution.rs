use super::*;

#[test]
fn workspace_cli_explicit_root_overrides_configured_and_relative_roots_use_cwd() {
    use std::ffi::OsStr;

    let directory = TempDir::new().expect("cwd");
    let cwd = directory.path();
    let explicit = cwd.join("explicit");
    let configured = cwd.join("configured");
    let cwd_id = create_root_fixture(cwd, "cwd fixture");
    let explicit_id = create_root_fixture(&explicit, "explicit fixture");
    let configured_id = create_root_fixture(&configured, "configured fixture");
    let nested = cwd.join("nested");
    let nested_id = create_root_fixture(&nested, "nested cwd fixture");

    let configured_read = cli_get_id(cwd, None, Some(configured.as_os_str()), &configured_id);
    assert_ok(&configured_read);
    let explicit_read = cli_get_id(
        cwd,
        Some(std::path::Path::new("explicit")),
        Some(configured.as_os_str()),
        &explicit_id,
    );
    assert_ok(&explicit_read);
    let explicit_with_empty_config = cli_get_id(
        cwd,
        Some(std::path::Path::new("explicit")),
        Some(OsStr::new("")),
        &explicit_id,
    );
    assert_ok(&explicit_with_empty_config);
    assert_eq!(
        cli_get_id(
            cwd,
            Some(std::path::Path::new("explicit")),
            Some(configured.as_os_str()),
            &configured_id
        )["status"],
        "error",
        "explicit relative workspace must override a conflicting configured root"
    );
    let cwd_read = cli_get_id(cwd, None, None, &cwd_id);
    assert_ok(&cwd_read);
    assert_ok(&cli_get_id(&nested, None, None, &nested_id));
    let missing_root = cwd.join("read-only-missing");
    let missing = cli_get_id(
        cwd,
        None,
        Some(std::path::Path::new("read-only-missing").as_os_str()),
        "00000000-0000-0000-0000-000000000000",
    );
    assert_eq!(missing["status"], "error");
    assert!(
        !missing_root
            .join(".workflow-tools/mission/missions")
            .exists()
    );
    let empty = cli_output(cwd, None, Some(OsStr::new("")), &["get".into(), cwd_id]);
    assert!(!empty.status.success());
    assert!(String::from_utf8_lossy(&empty.stderr).contains("MISSION_WORKSPACE must not be empty"));
}

#[test]
fn workspace_resolver_selects_portable_canonical_root_and_rejects_symlinks() {
    use mission::resolve_workspace;
    use std::{ffi::OsStr, path::Path};

    let checkout = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(4)
        .expect("meta-workspace root");
    let (canonical, source) =
        resolve_workspace(None, Some(OsStr::new("workflow-tools/mission")), checkout)
            .expect("canonical relative launch root");
    assert_eq!(source, "configured");
    assert_eq!(canonical, checkout.join("workflow-tools/mission"));

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let directory = TempDir::new().expect("temporary root");
        let real = directory.path().join("real");
        std::fs::create_dir(&real).unwrap();
        let link = directory.path().join("link");
        symlink(&real, &link).unwrap();
        let error = resolve_workspace(Some(&link), None, directory.path()).unwrap_err();
        assert!(error.contains("must not contain symlinks"));
    }
}

#[test]
fn workspace_mcp_explicit_configured_and_cwd_roots_are_selected() {
    use std::ffi::OsStr;

    let directory = TempDir::new().expect("cwd");
    let cwd = directory.path();
    let explicit = cwd.join("explicit");
    let configured = cwd.join("configured");
    let nested = cwd.join("nested");
    let explicit_id = create_root_fixture(&explicit, "MCP explicit fixture");
    let configured_id = create_root_fixture(&configured, "MCP configured fixture");
    let nested_id = create_root_fixture(&nested, "MCP cwd fixture");

    let mut explicit_mcp = Mcp::start_with(
        cwd,
        Some(std::path::Path::new("explicit")),
        Some(configured.as_os_str()),
    );
    explicit_mcp.initialize();
    let tools = explicit_mcp.request(2, "tools/list", json!({}));
    let name = tools["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "mission_operation")
        .unwrap()["name"]
        .as_str()
        .expect("compatibility tool name");
    assert_eq!(
        explicit_mcp.call(
            3,
            name,
            json!({"operation":"get", "mission_id":explicit_id})
        )["status"],
        "ok"
    );
    assert_eq!(
        explicit_mcp.call(
            4,
            name,
            json!({"operation":"get", "mission_id":configured_id})
        )["status"],
        "error"
    );

    let mut configured_mcp = Mcp::start_with(cwd, None, Some(OsStr::new("configured")));
    configured_mcp.initialize();
    let tools = configured_mcp.request(2, "tools/list", json!({}));
    let name = tools["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "mission_operation")
        .unwrap()["name"]
        .as_str()
        .expect("compatibility tool name");
    assert_eq!(
        configured_mcp.call(
            3,
            name,
            json!({"operation":"get", "mission_id":configured_id})
        )["status"],
        "ok"
    );

    let mut cwd_mcp = Mcp::start(&nested);
    cwd_mcp.initialize();
    let tools = cwd_mcp.request(2, "tools/list", json!({}));
    let name = tools["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "mission_operation")
        .unwrap()["name"]
        .as_str()
        .expect("compatibility tool name");
    assert_eq!(
        cwd_mcp.call(3, name, json!({"operation":"get", "mission_id":nested_id}))["status"],
        "ok"
    );
}
