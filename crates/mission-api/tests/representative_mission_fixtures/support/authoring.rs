use super::*;

#[derive(Clone, Copy)]
pub(crate) struct FixtureIdentity {
    pub(crate) mission: u128,
    pub(crate) ticket: u128,
    pub(crate) spec: u128,
    pub(crate) criterion: &'static str,
    execution: &'static str,
    title: &'static str,
}

pub(crate) const DOC_IDENTITY: FixtureIdentity = FixtureIdentity {
    mission: 0x00000000000040008000000000001001,
    ticket: 0x00000000000040008000000000001002,
    spec: 0x00000000000040008000000000001003,
    criterion: "doc-comment-review-contract",
    execution: "doc-comment-synthetic-check",
    title: "Synthetic documentation workflow fixture",
};
pub(crate) const MISSION_IDENTITY: FixtureIdentity = FixtureIdentity {
    mission: 0x00000000000040008000000000002001,
    ticket: 0x00000000000040008000000000002002,
    spec: 0x00000000000040008000000000002003,
    criterion: "mission-domain-boundary",
    execution: "mission-integration-synthetic-check",
    title: "Synthetic Mission integration fixture",
};
pub(crate) const TICKET_IDENTITY: FixtureIdentity = FixtureIdentity {
    mission: 0x00000000000040008000000000003001,
    ticket: 0x00000000000040008000000000003002,
    spec: 0x00000000000040008000000000003003,
    criterion: "ticket-migration-safety",
    execution: "ticket-reliability-synthetic-check",
    title: "Synthetic Ticket reliability fixture",
};

#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct NativeFixtureInput {
    pub(crate) mission_id: Uuid,
    pub(crate) bundle: MissionBundle,
}

pub(crate) fn outcome(seed: &Seed) -> WaypointOutcomePlan {
    WaypointOutcomePlan {
        expected_outcome: seed.expected.into(),
        alternative_outcomes: vec![AlternativeOutcome {
            outcome: seed.alternative.into(),
            acceptable_handling: vec![
                "Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.".into(),
            ],
        }],
    }
}

pub(crate) fn part_markdown(seed: &Seed) -> String {
    format!(
        "## Adapted objective\n\n{}\n\n## Constraints and boundary\n\n{}\n\n## Validation and non-goal\n\nValidation guidance: {}. This is a planned check, not a claim that the source command was run.\n\nNon-goal: {}.\n\n## Fixture-authored outcome guidance\n\nExpected: {}\n\nAlternative: {}\n\nAcceptable handling: record the condition, preserve source data, and stop affected follow-up.\n",
        seed.objective,
        seed.constraints,
        seed.validation,
        seed.non_goal,
        seed.expected,
        seed.alternative
    )
}

pub(crate) fn fixture_relationships(identity: FixtureIdentity) -> WaypointRelationships {
    WaypointRelationships {
        specification_ids: vec![Uuid::from_u128(identity.spec)],
        criteria: vec![CriterionReference {
            specification_id: Uuid::from_u128(identity.spec),
            criterion_id: CriterionId::new(identity.criterion).unwrap(),
        }],
        executions: vec![WaypointExecutionReference {
            execution_id: identity.execution.into(),
            criterion: None,
        }],
    }
}

pub(crate) fn native_input(
    identity: FixtureIdentity,
    seeds: &[Seed],
    completed: bool,
    blocked_index: Option<usize>,
    pending_index: Option<usize>,
    linked_index: usize,
) -> NativeFixtureInput {
    let ticket_ref = format!("ce://default/ticket/{}", Uuid::from_u128(identity.ticket));
    let waypoints = seeds
        .iter()
        .enumerate()
        .map(|(index, seed)| {
            let linked = index == linked_index;
            WaypointFragment {
                outcome_plan: Some(outcome(seed)),
                waypoint_id: seed.id.into(),
                title: seed.title.into(),
                depends_on: index
                    .checked_sub(1)
                    .map(|previous| vec![seeds[previous].id.into()])
                    .unwrap_or_default(),
                mode: WaypointMode::ExecutionSideEffect,
                scope: if linked {
                    Scope::TicketBacked {
                        ticket_ref: ticket_ref.clone(),
                    }
                } else {
                    Scope::SingleSession
                },
                state: if linked {
                    None
                } else if blocked_index == Some(index) {
                    Some(WaypointState::Blocked)
                } else if pending_index == Some(index) {
                    Some(WaypointState::Pending)
                } else if completed {
                    Some(WaypointState::Completed)
                } else {
                    None
                },
                ticket_ref: linked.then(|| ticket_ref.clone()),
                child_mission_ref: None,
                relationships: if linked {
                    fixture_relationships(identity)
                } else {
                    WaypointRelationships::default()
                },
                session_package: format!("w8-{}", seed.id),
                prompt: format!(
                    "Fixture-only adaptation: {} Preserve the boundary: {}",
                    seed.objective, seed.constraints
                ),
                artifacts: vec![],
                requirement_ids: vec![],
                non_goals: vec![seed.non_goal.into()],
                validation: vec![ValidationGate {
                    command: seed.validation.into(),
                    cwd: None,
                }],
                commit_checkpoint: CommitCheckpoint {
                    message: format!("Fixture review checkpoint: {}", seed.id),
                },
                part_markdown: part_markdown(seed),
            }
        })
        .collect();
    NativeFixtureInput {
        mission_id: Uuid::from_u128(identity.mission),
        bundle: MissionBundle {
            manifest: BundleManifest {
                schema_version: OUTCOME_BUNDLE_SCHEMA_VERSION,
                revision: 1,
                title: identity.title.into(),
                objective: format!(
                    "Synthetic, read-only adaptation of {} waypoints. No source roadmap is executed or mutated.",
                    seeds.len()
                ),
                waypoint_files: vec![],
                execution_order: seeds.iter().map(|seed| seed.id.into()).collect(),
                artifacts: vec![],
                requirements: vec![],
                validation_gates: vec![],
                notes: vec![
                    "Synthetic adaptation. Mission API predecessor checkpoints encode rendered order and do not assert source-roadmap prerequisites.".into(),
                    "Fixture-authored outcome plans and synthetic local evidence are not requester approvals or real execution evidence.".into(),
                ],
                operator_results: if identity.mission == DOC_IDENTITY.mission {
                    vec![OperatorResult {
                        waypoint_id: "code-map".into(),
                        outcome: "passed".into(),
                        operator_command: "fixture-authored observation; source workspace was not scanned".into(),
                        evidence_pointer: format!(
                            "ce://default/test/{}",
                            DOC_IDENTITY.execution
                        ),
                    }]
                } else {
                    vec![]
                },
            },
            waypoints,
        },
    }
}

pub(crate) fn typed_draft(identity: FixtureIdentity) -> TypedDraft {
    let ticket_id = Uuid::from_u128(identity.ticket);
    TypedDraft {
        mission_id: Uuid::from_u128(identity.mission),
        schema_version: OUTCOME_SOURCE_SCHEMA_VERSION,
        revision: 1,
        title: identity.title.into(),
        objective: "Synthetic adaptation of the full Mission integration roadmap; no source work is executed or mutated.".into(),
        waypoints: MISSION_SEEDS
            .iter()
            .map(|seed| TypedWaypoint {
                outcome_plan: Some(outcome(seed)),
                waypoint_id: seed.id.into(),
                title: seed.title.into(),
                prompt: format!(
                    "Fixture-only adaptation: {} Preserve the boundary: {}",
                    seed.objective, seed.constraints
                ),
                part: PartSource {
                    waypoint_id: seed.id.into(),
                    document: Document {
                        blocks: vec![
                            Block::Heading {
                                level: 2,
                                inlines: vec![Inline::Text {
                                    text: "Adapted objective".into(),
                                }],
                            },
                            Block::Paragraph {
                                inlines: vec![Inline::Text {
                                    text: seed.objective.into(),
                                }],
                            },
                            Block::Heading {
                                level: 2,
                                inlines: vec![Inline::Text {
                                    text: "Constraints and validation".into(),
                                }],
                            },
                            Block::Paragraph {
                                inlines: vec![Inline::Text {
                                    text: format!(
                                        "{} Validation guidance: {}. Non-goal: {}.",
                                        seed.constraints, seed.validation, seed.non_goal
                                    ),
                                }],
                            },
                        ],
                    },
                },
                depends_on: vec![],
                validation: vec![ValidationGate {
                    command: seed.validation.into(),
                    cwd: None,
                }],
                commit_checkpoint: CommitCheckpoint {
                    message: format!("Fixture review checkpoint: {}", seed.id),
                },
                non_goals: vec![seed.non_goal.into()],
                ownership_refs: if seed.id == "mission-api" {
                    OwnershipRefs {
                        ticket_id: Some(ticket_id),
                        ..Default::default()
                    }
                } else {
                    OwnershipRefs::default()
                },
                relationships: if seed.id == "mission-api" {
                    fixture_relationships(identity)
                } else {
                    WaypointRelationships::default()
                },
            })
            .collect(),
    }
}
