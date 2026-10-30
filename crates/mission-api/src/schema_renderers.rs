//! Bounded read-only schema projections and canonical criterion references.

use super::*;
use spec_api::{AcceptanceCriterion, CriterionArtifact, SpecManifest};
use ticket_api::model::ticket::TicketManifest;

/// Version of the read-only intersection, not a Ticket storage version.
pub const SHARED_SCHEMA_VERSION: u32 = 1;

/// Proven source formats. Ticket does not declare a native schema version.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SharedSourceSchema {
    MissionBundleV1,
    MissionBundleV2,
    MissionSourceV2,
    MissionSourceV3,
    TicketUnversioned,
}

/// Only UUID identity and a nonblank string title have common semantics.
/// Lifecycle state, revision, timestamps and arbitrary extras stay domain-owned.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharedEntity {
    pub schema_version: u32,
    pub source_schema: SharedSourceSchema,
    pub id: Uuid,
    pub title: String,
}

/// Failed decoding retains the exact input, including unknown fields.
#[derive(Debug, Error)]
#[error("{diagnostic:?}")]
pub struct SharedSchemaError {
    pub diagnostic: Box<Diagnostic>,
    pub source_bytes: Vec<u8>,
}

impl SharedEntity {
    /// Strict decoding; unsupported formats never coerce or migrate on read.
    pub fn from_json(source: &[u8]) -> Result<Self, SharedSchemaError> {
        let failure = |diagnostic| SharedSchemaError {
            diagnostic: Box::new(diagnostic),
            source_bytes: source.to_vec(),
        };
        let value: Self = serde_json::from_slice(source).map_err(|error| {
            failure(contract_diag(
                "mission-shared-schema-malformed",
                "",
                &format!("Invalid shared projection: {error}. Preserve input and use its owning domain's explicit migration."),
            ))
        })?;
        value.validate().map_err(failure)?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), Diagnostic> {
        if self.schema_version != SHARED_SCHEMA_VERSION {
            return Err(contract_diag(
                "mission-shared-schema-unsupported",
                "/schema_version",
                "Only shared projection v1 is supported. Preserve input; migrate through its owning domain, then project again.",
            ));
        }
        if self.id.is_nil() || self.title.trim().is_empty() {
            return Err(contract_diag(
                "mission-shared-schema-invalid",
                "/id,title",
                "Identity must be a non-nil UUID and title a nonblank string.",
            ));
        }
        Ok(())
    }

    pub fn from_mission(mission: &AcceptedMission) -> Result<Self, Diagnostic> {
        if !matches!(
            mission.bundle.manifest.schema_version,
            SCHEMA_VERSION | OUTCOME_BUNDLE_SCHEMA_VERSION
        ) {
            return Err(contract_diag(
                "mission-shared-schema-unsupported",
                "/bundle/manifest/schema_version",
                "Expected native Mission v1 or v2; use the existing journaled Mission migration for supported conversions, never version coercion.",
            ));
        }
        for waypoint in &mission.bundle.waypoints {
            outcome_plan::validate_outcome_plan(
                waypoint.outcome_plan.as_ref(),
                mission.bundle.manifest.schema_version == OUTCOME_BUNDLE_SCHEMA_VERSION,
                &waypoint.waypoint_id,
            )?;
        }
        Self::project(
            mission.mission_id,
            &mission.bundle.manifest.title,
            if mission.bundle.manifest.schema_version == OUTCOME_BUNDLE_SCHEMA_VERSION {
                SharedSourceSchema::MissionBundleV2
            } else {
                SharedSourceSchema::MissionBundleV1
            },
        )
    }

    pub fn from_typed_mission(source: &TypedDraft) -> Result<Self, Diagnostic> {
        if !matches!(
            source.schema_version,
            TYPED_SOURCE_SCHEMA_VERSION | OUTCOME_SOURCE_SCHEMA_VERSION
        ) {
            return Err(contract_diag(
                "mission-shared-schema-unsupported",
                "/schema_version",
                "Expected typed Mission source v2 or v3; preserve source and use the owning migration API.",
            ));
        }
        for waypoint in &source.waypoints {
            outcome_plan::validate_outcome_plan(
                waypoint.outcome_plan.as_ref(),
                source.schema_version == OUTCOME_SOURCE_SCHEMA_VERSION,
                &waypoint.waypoint_id,
            )?;
        }
        Self::project(
            source.mission_id,
            &source.title,
            if source.schema_version == OUTCOME_SOURCE_SCHEMA_VERSION {
                SharedSourceSchema::MissionSourceV3
            } else {
                SharedSourceSchema::MissionSourceV2
            },
        )
    }

    pub fn from_ticket(ticket: &TicketManifest) -> Result<Self, Diagnostic> {
        let title = ticket
            .extra
            .get("title")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                contract_diag(
                    "mission-shared-schema-invalid",
                    "/title",
                    "Ticket title must be a string.",
                )
            })?;
        Self::project(ticket.id, title, SharedSourceSchema::TicketUnversioned)
    }

    fn project(
        id: Uuid,
        title: &str,
        source_schema: SharedSourceSchema,
    ) -> Result<Self, Diagnostic> {
        let value = Self {
            schema_version: SHARED_SCHEMA_VERSION,
            source_schema,
            id,
            title: title.into(),
        };
        value.validate()?;
        Ok(value)
    }
}

/// The criterion ID is scoped to the canonical specification UUID.
/// Resolution is deliberately local to an explicitly selected owning workspace.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CriterionReference {
    #[schemars(with = "String")]
    pub specification_id: Uuid,
    pub criterion_id: CriterionId,
}

/// Opaque Spec-owned stable ID; unlike a Requirement ID it is not a Mission slug.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct CriterionId(String);

impl CriterionId {
    pub fn new(value: impl Into<String>) -> Result<Self, Diagnostic> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(contract_diag(
                "mission-requirement-reference-invalid",
                "/criterion_id",
                "A canonical Spec criterion ID must be nonblank.",
            ));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl CriterionReference {
    pub fn specification_uri(&self) -> String {
        format!("ce://default/spec/{}", self.specification_id)
    }

    pub fn validate(&self) -> Result<(), Diagnostic> {
        if self.specification_id.is_nil() || self.criterion_id.as_str().trim().is_empty() {
            return Err(contract_diag(
                "mission-requirement-reference-invalid",
                "/canonical_criteria",
                "A reference requires a non-nil specification UUID and a nonblank Spec-owned criterion ID.",
            ));
        }
        Ok(())
    }

    /// Reads only the exact canonical Spec path. No ancestor/index fallback,
    /// store initialization, copied criterion text, or criterion mutation.
    pub fn resolve(&self, workspace: &Path) -> Result<ResolvedCriterion, MissionError> {
        self.validate().map_err(one_diagnostic)?;
        let relative = Path::new(".workflow-tools/spec/specs")
            .join(self.specification_id.to_string())
            .join("spec.toml");
        let path = workspace.join(&relative);
        resolve_artifact_reference(workspace, Path::new("criterion-consumer.md"), &relative.to_string_lossy())
            .map_err(|_| one_diagnostic(contract_diag(
                "mission-requirement-spec-unavailable", &path.to_string_lossy(),
                "Canonical specification is missing or escapes the explicitly selected workspace; no other root is searched.",
            )))?;
        if path.canonicalize()? != workspace.canonicalize()?.join(&relative) {
            return Err(one_diagnostic(contract_diag(
                "mission-requirement-spec-wrong-root",
                &path.to_string_lossy(),
                "Canonical Spec paths must not redirect through symlinks, including nested stores inside this workspace.",
            )));
        }
        let spec: SpecManifest = toml::from_str(&fs::read_to_string(&path)?).map_err(|error| {
            one_diagnostic(contract_diag(
                "mission-requirement-spec-malformed",
                &path.to_string_lossy(),
                &error.to_string(),
            ))
        })?;
        if spec.id != self.specification_id {
            return Err(one_diagnostic(contract_diag(
                "mission-requirement-spec-identity-mismatch",
                &path.to_string_lossy(),
                "Canonical manifest identity must match its specification directory.",
            )));
        }
        let (field, matches) = if spec.is_v2() {
            let criteria: Vec<CriterionArtifact> = strict_criteria(&spec, "criteria")?;
            (
                "criteria",
                criteria
                    .into_iter()
                    .filter(|c| c.criterion_id == self.criterion_id.as_str())
                    .map(CriterionValue::Artifact)
                    .collect::<Vec<_>>(),
            )
        } else {
            if spec.extra.contains_key("format_version") {
                return Err(one_diagnostic(contract_diag(
                    "mission-requirement-spec-version-unsupported",
                    &path.to_string_lossy(),
                    "Only unversioned legacy Specs or explicit Spec format v2 are supported; preserve the source.",
                )));
            }
            let criteria: Vec<AcceptanceCriterion> = strict_criteria(&spec, "acceptance_criteria")?;
            (
                "acceptance_criteria",
                criteria
                    .into_iter()
                    .filter(|c| c.id == self.criterion_id.as_str())
                    .map(CriterionValue::Acceptance)
                    .collect::<Vec<_>>(),
            )
        };
        if matches.len() != 1 {
            return Err(one_diagnostic(contract_diag(
                "mission-requirement-criterion-unresolved",
                field,
                "Criterion must exist exactly once in the referenced specification, not in another specification or root.",
            )));
        }
        matches
            .into_iter()
            .next()
            .map(|value| ResolvedCriterion {
                specification_id: spec.id,
                value,
            })
            .ok_or_else(|| {
                one_diagnostic(contract_diag(
                    "mission-requirement-criterion-unresolved",
                    field,
                    "Criterion is unavailable.",
                ))
            })
    }
}

/// A read-only resolution retains its owning specification identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedCriterion {
    specification_id: Uuid,
    value: CriterionValue,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum CriterionValue {
    Acceptance(AcceptanceCriterion),
    Artifact(CriterionArtifact),
}

fn strict_criteria<T: serde::de::DeserializeOwned>(
    spec: &SpecManifest,
    field: &str,
) -> Result<Vec<T>, MissionError> {
    serde_json::from_value(
        spec.extra
            .get(field)
            .cloned()
            .unwrap_or_else(|| serde_json::json!([])),
    )
    .map_err(|error| {
        one_diagnostic(contract_diag(
            "mission-requirement-spec-malformed",
            field,
            &error.to_string(),
        ))
    })
}

pub(super) fn validate_requirement_shapes(requirements: &[Requirement]) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for requirement in requirements {
        let mut references = BTreeSet::new();
        for reference in &requirement.canonical_criteria {
            if let Err(diagnostic) = reference.validate() {
                diagnostics.push(diagnostic);
            }
            if !references.insert(reference) {
                diagnostics.push(contract_diag(
                    "mission-requirement-reference-duplicate",
                    "/canonical_criteria",
                    "Each canonical criterion reference must be listed once.",
                ));
            }
        }
    }
    diagnostics
}

impl MissionStore {
    pub(super) fn validate_requirement_references(
        &self,
        bundle: &MissionBundle,
    ) -> Result<(), MissionError> {
        let workspace = self.root.ancestors().nth(3).unwrap_or(&self.root);
        for requirement in &bundle.manifest.requirements {
            for reference in &requirement.canonical_criteria {
                reference.resolve(workspace)?;
            }
        }
        Ok(())
    }
}

/// Render a standalone Mission identity block, without loading external state.
mod rendering;

pub(crate) use rendering::{contract_diag, markdown_text, one_diagnostic};
pub use rendering::{
    render_criterion_entity, render_criterion_reference, render_mission_entity,
    render_requirement_entity, render_requirement_row, render_roadmap_entity,
    render_specification_entity, render_ticket_entity,
};
