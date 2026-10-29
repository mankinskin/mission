use super::*;

pub(super) fn label(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('[', "&#91;")
        .replace(']', "&#93;")
        .replace(['\n', '\r'], " ")
}

fn encode_path(value: &str) -> String {
    value.bytes().fold(String::new(), |mut output, byte| {
        if byte.is_ascii_alphanumeric() || b"/-._~".contains(&byte) {
            output.push(char::from(byte));
        } else {
            output.push_str(&format!("%{byte:02X}"));
        }
        output
    })
}

pub(super) fn render_reference(
    reference: &str,
    workspace: Option<&Path>,
    consumer: Option<&Path>,
) -> String {
    let resolved = if reference.starts_with("ce://") {
        if artifact_contract::validate_canonical_uri(reference) {
            Ok(encode_urn(reference))
        } else {
            Err("mission-reference-unknown")
        }
    } else if let (Some(workspace), Some(consumer)) = (workspace, consumer) {
        resolve_artifact_reference(workspace, consumer, reference)
            .map_err(|diagnostic| diagnostic.code)
    } else {
        Err("mission-reference-context-unavailable")
    };
    match resolved {
        Ok(target) => format!("[{}]({target})", label(reference)),
        Err(code) => format!("{}: unavailable ({code})", label(reference)),
    }
}

pub(super) fn render(
    bundle: &MissionBundle,
    mission_id: Uuid,
    snapshot_id: Option<&str>,
    workspace: Option<&Path>,
    consumer: Option<&Path>,
) -> Vec<u8> {
    let mut output = format!(
        "# Mission review entry\n\nMission: `{mission_id}`; revision: {}.\n\n## Promise\n\n{}\n\nFull contract and requirements: [Roadmap](ROADMAP.md); authority: [accepted Mission](../accepted.json).\n\n## Delivery\n\nGenerated projections (not proof of acceptance):\n\n- [Review entry](README.md)\n- [Roadmap](ROADMAP.md)\n",
        bundle.manifest.revision,
        label(&bundle.manifest.objective),
    );
    for (index, id) in bundle.manifest.execution_order.iter().enumerate() {
        let name = format!("{:02}-{id}.md", index + 1);
        output.push_str(&format!("- [W{}]({})\n", index + 1, encode_path(&name)));
    }
    output.push_str("\n## Declared evidence\n\n");
    let references: BTreeSet<&str> = bundle
        .manifest
        .artifacts
        .iter()
        .chain(
            bundle
                .waypoints
                .iter()
                .flat_map(|waypoint| &waypoint.artifacts),
        )
        .map(String::as_str)
        .chain(
            bundle
                .manifest
                .operator_results
                .iter()
                .map(|result| result.evidence_pointer.as_str()),
        )
        .collect();
    if references.is_empty() {
        output.push_str("Evidence: unknown (none declared).\n");
    }
    for reference in references {
        output.push_str(&format!(
            "- {}\n",
            render_reference(reference, workspace, consumer)
        ));
    }
    output.push_str("\n## Verification\n\n- What was promised? See the roadmap's objective and requirements.\n- What was delivered? Follow the generated inventory and declared evidence above.\n- What was verified? See the roadmap's validation gates and operator results; missing results are unknown.\n\n");
    output.push_str(&format!(
        "External status: `{}`.\n",
        label(snapshot_id.unwrap_or("{}"))
    ));
    output.push_str("\nPublication transport success is not acceptance. Product readiness and historical review acceptance are unknown unless explicitly evidenced. Run `mission check-generated` for current file and destination diagnostics; this entry does not assert that verification passed.\n");
    output.into_bytes()
}

pub(super) fn encode_urn(reference: &str) -> String {
    reference
        .replace('%', "%25")
        .replace('(', "%28")
        .replace(')', "%29")
        .replace(' ', "%20")
        .replace('\n', "%0A")
        .replace('\r', "%0D")
}

pub(super) fn check_generated(
    rendered: &RenderedRoadmap,
    generated: &Path,
) -> Result<Vec<Diagnostic>, MissionError> {
    let mut diagnostics = Vec::new();
    let root = generated
        .ancestors()
        .nth(5)
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Generated directory must have an owning workspace",
            )
        })?
        .canonicalize()?;
    match fs::symlink_metadata(generated) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Ok(vec![issue(
                "generated-destination-invalid",
                generated,
                "Generated output must be a regular directory, not a symbolic link or file.",
            )]);
        }
        Ok(_) if !generated.canonicalize()?.starts_with(&root) => {
            return Ok(vec![issue(
                "generated-destination-invalid",
                generated,
                "Generated output directory escapes its owning workspace.",
            )]);
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let expected: BTreeMap<_, _> = rendered.files().collect();
    let mut published_readme = None;
    for (name, bytes) in &expected {
        let path = generated.join(name);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                let code = match path.canonicalize() {
                    Ok(_) => "generated-destination-invalid",
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        "generated-dangling"
                    }
                    Err(error) => return Err(error.into()),
                };
                diagnostics.push(issue(
                    code,
                    &path,
                    "Generated files must be regular files, not symbolic links.",
                ));
            }
            Ok(metadata) if !metadata.is_file() => diagnostics.push(issue(
                "generated-destination-invalid",
                &path,
                "Expected a generated regular file.",
            )),
            Ok(_) => {
                let contents = fs::read(&path)?;
                if contents != *bytes {
                    diagnostics.push(issue(
                        "generated-changed",
                        &path,
                        "Generated bytes differ from the accepted projection.",
                    ));
                }
                if *name == "README.md" {
                    published_readme = Some(contents);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => diagnostics.push(issue(
                "generated-missing",
                &path,
                "Expected generated file is missing.",
            )),
            Err(error) => return Err(error.into()),
        }
    }
    match fs::read_dir(generated) {
        Ok(entries) => {
            let mut entries = entries.collect::<Result<Vec<_>, _>>()?;
            entries.sort_by_key(|entry| entry.file_name());
            for entry in entries {
                if !expected.contains_key(entry.file_name().to_string_lossy().as_ref()) {
                    diagnostics.push(issue(
                        "generated-extra",
                        &entry.path(),
                        "File is not in the accepted generated inventory.",
                    ));
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    // Filesystem changes can invalidate links still present in the published entry.
    for target in String::from_utf8_lossy(published_readme.as_deref().unwrap_or(&rendered.readme))
        .split("](")
        .skip(1)
        .filter_map(|value| value.split(')').next())
    {
        if target.starts_with("ce://") {
            continue;
        }
        let destination = generated.join(decode_path(target)?);
        match destination.canonicalize() {
            Ok(path) if path.starts_with(&root) => {}
            Ok(_) => diagnostics.push(issue(
                "generated-destination-invalid",
                &destination,
                "Review-entry destination escapes its workspace.",
            )),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => diagnostics.push(issue(
                "generated-dangling",
                &destination,
                "Review-entry destination does not exist.",
            )),
            Err(error) => return Err(error.into()),
        }
    }
    Ok(diagnostics)
}

fn decode_path(value: &str) -> Result<String, MissionError> {
    let mut bytes = Vec::new();
    let mut input = value.as_bytes().iter().copied();
    while let Some(byte) = input.next() {
        if byte == b'%' {
            let high = input.next().and_then(|byte| char::from(byte).to_digit(16));
            let low = input.next().and_then(|byte| char::from(byte).to_digit(16));
            match (high, low) {
                (Some(high), Some(low)) => bytes.push((high * 16 + low) as u8),
                _ => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "Invalid percent-encoded review-entry destination",
                    )
                    .into());
                }
            }
        } else {
            bytes.push(byte);
        }
    }
    String::from_utf8(bytes)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error).into())
}

fn issue(code: &'static str, path: &Path, message: &str) -> Diagnostic {
    Diagnostic {
        code,
        source: path.display().to_string(),
        pointer: String::new(),
        waypoint_id: None,
        message: message.into(),
    }
}
