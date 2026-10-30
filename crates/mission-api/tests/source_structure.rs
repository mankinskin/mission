//! Structural acceptance checks for the four workflow implementation roots.
use std::{path::Path, process::Command};

const SOURCE_EXTENSIONS: &[&str] = &[
    "rs", "ts", "tsx", "js", "jsx", "mjs", "cjs", "py", "sh", "bash", "ps1", "vue", "css", "scss",
    "html", "sql", "wgsl", "c", "h", "cpp", "hpp", "go",
];

#[test]
fn workflow_sources_and_tests_fit_the_500_line_limit() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("mission-api belongs to the workflow-tools checkout");
    let mut violations = Vec::new();
    for domain in ["mission", "ticket", "session", "memory-kernel"] {
        let root = workspace.join(domain);
        let output = Command::new("git")
            .arg("-C")
            .arg(&root)
            .args([
                "ls-files",
                "-z",
                "--cached",
                "--others",
                "--exclude-standard",
                ".",
            ])
            .output()
            .expect("git is required to enumerate maintained sources");
        assert!(
            output.status.success(),
            "source inventory failed for {domain}"
        );
        let paths = std::str::from_utf8(&output.stdout).expect("source paths must be UTF-8");
        let mut files = paths
            .split('\0')
            .filter(|path| !path.is_empty())
            .collect::<Vec<_>>();
        files.sort_unstable();
        files.dedup();
        let mut checked = 0;
        for relative in files {
            let path = root.join(relative);
            let extension = path.extension().and_then(|value| value.to_str());
            if !extension.is_some_and(|value| SOURCE_EXTENSIONS.contains(&value)) || !path.is_file()
            {
                continue;
            }
            let source = std::fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
            checked += 1;
            let lines = source.lines().count();
            if lines > 500 {
                violations.push(format!("{domain}/{relative}: {lines} lines"));
            }
        }
        assert!(
            checked > 0,
            "source inventory for {domain} must not be empty"
        );
    }
    assert!(
        violations.is_empty(),
        "split by responsibility; sources must have at most 500 lines:\n{}",
        violations.join("\n")
    );
}
