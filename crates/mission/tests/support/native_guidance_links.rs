use markdown_links::extract;

const PRODUCT: &str = "https://github.com/mankinskin/workflow-tools/blob/main";

pub fn assert_contract(read: &impl Fn(&str) -> String, sources: &[&str]) {
    let intent = read(sources[3]);
    let authoring = read(sources[4]);
    let execution = read(sources[5]);
    let tooling = read(sources[6]);
    for (text, fragment) in [
        (&intent, "intent-authority"),
        (&authoring, "intent-authority"),
        (&authoring, "provider-evidence"),
        (&execution, "provider-evidence"),
        (&execution, "validation-features"),
        (&tooling, "intent-authority"),
        (&tooling, "provider-evidence"),
    ] {
        assert!(
            extract(text)
                .iter()
                .any(|link| link.destination.ends_with(&format!("README.md#{fragment}"))),
            "guidance must enter product docs through the allowed README index: {fragment}"
        );
    }
    let index = read("workflow-tools/mission/docs/README.md");
    for destination in [
        "intent-authority.md",
        "provider-evidence.md",
        "provider-evidence.md#validation-features",
    ] {
        assert!(
            extract(&index)
                .iter()
                .any(|link| { link.destination.ends_with(destination) }),
            "README index must preserve the exact canonical body: {destination}"
        );
    }
    for (source, destinations) in [
        (
            sources[7],
            vec![
                ".agents/instructions/workflow/roadmap-authoring.instructions.md#source-to-outcome-fidelity",
                ".agents/instructions/workflow/roadmap-execution.instructions.md#native-evidence-and-failure-boundary",
                "mission/docs/provider-evidence.md",
            ],
        ),
        (
            sources[8],
            vec![
                ".agents/instructions/workflow/roadmap-authoring.instructions.md#source-to-outcome-fidelity",
                "mission/docs/provider-evidence.md",
            ],
        ),
    ] {
        let text = read(source);
        for destination in destinations {
            let canonical = format!("{PRODUCT}/{destination}");
            assert!(
                extract(&text)
                    .iter()
                    .any(|link| link.destination == canonical),
                "{source}: upward repository references require the exact canonical URL {canonical}"
            );
        }
    }
}
