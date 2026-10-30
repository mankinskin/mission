use super::*;

pub(crate) fn history(root: Option<&Path>, revision: u64) -> Result<Vec<u8>, MissionError> {
    let mut text = String::from(
        "# Mission output history\n\nRevision labels identify whole states, not individual files. Missing states are never reconstructed from today's renderer. Human reactions are separate records, not publication locks or acceptance inferred from publication.\n\n",
    );
    // Render ranges rather than iterating unbounded missing revision numbers.
    let mut available = BTreeSet::new();
    if let Some(root) = root {
        let states = root.join("history/states");
        if states.exists() {
            typed_source::ensure_safe_path(root, &states)?;
            for entry in fs::read_dir(states)? {
                let entry = entry?;
                let number = entry
                    .file_name()
                    .to_str()
                    .and_then(|name| name.strip_prefix("revision-"))
                    .and_then(|number| number.parse::<u64>().ok())
                    .ok_or_else(|| {
                        invalid("Archived states require canonical revision-N labels.")
                    })?;
                if number == 0
                    || entry.file_name().to_str() != Some(format!("revision-{number}").as_str())
                    || !entry.file_type()?.is_dir()
                {
                    return Err(invalid(
                        "Archived state labels must be unique canonical directories, not aliases or links.",
                    ));
                }
                if number < revision && entry.path().join("output-contract.json").is_file() {
                    validate_contract(&entry.path(), self_id(root)?, number)?;
                    available.insert(number);
                }
            }
        }
    }
    available.insert(revision);
    let mut next = 1;
    for number in available {
        if number > next {
            if number == next + 1 {
                text.push_str(&format!(
                    "- revision-{next}: unrecoverable gap (no complete archived state).\n"
                ));
            } else {
                text.push_str(&format!("- revision-{next}..revision-{}: unrecoverable gap (no complete archived states).\n", number - 1));
            }
        }
        text.push_str(&format!("- [revision-{number}](../history/states/revision-{number}/output-contract.json): available complete state{}.\n",
            if number == revision { " (current)" } else { "" }));
        next = number.saturating_add(1);
    }
    text.push_str("\nReactions, when recorded: `../history/events/revision-N.json`. Missing reactions mean unknown; approvals are never reused for a later revision.\n");
    Ok(text.into_bytes())
}

pub(super) fn self_id(root: &Path) -> Result<Uuid, MissionError> {
    let accepted: AcceptedMission = serde_json::from_slice(&fs::read(root.join("accepted.json"))?)?;
    Ok(accepted.mission_id)
}
