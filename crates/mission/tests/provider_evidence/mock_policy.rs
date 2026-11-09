use super::*;
use mission::intent::{IntentOperation, IntentOutput, execute_intent};
use session_api::HostPromptIngress;

fn confirmed_behavior(allow_mock: bool) -> approval_fixture::Fx {
    let f =
        approval_fixture::linked_with_input(b"Implement behavior with explicit evidence policy");
    f.attach_execution_contracts();
    let source = b"Implement behavior with explicit evidence policy";
    let requirements = vec![BaselineRequirementV4 {
        requirement_id: "behavior".into(),
        source: IntentSourceSpan {
            source_sha256: intent_source_digest(source),
            start: 0,
            end: source.len(),
            excerpt: String::from_utf8(source.to_vec()).unwrap(),
        },
        outcome: "Working behavior".into(),
        category: IntentOutcomeCategory::WorkingBehavior,
        evidence_policy: IntentEvidencePolicy::ExecutableBehavior { allow_mock },
    }];
    let reference = IntentReceiptReference {
        session_id: approval_fixture::SESSION.into(),
        challenge_id: Uuid::new_v4(),
    };
    let revision = f.plan().revision;
    let (_, output) = execute_intent(
        &f.ws,
        f.id,
        IntentOperation::PrepareBaseline {
            expected_revision: revision,
            requirements: requirements.clone(),
            reference: reference.clone(),
        },
    )
    .unwrap();
    let IntentOutput::Challenge { prompt, .. } = output else {
        panic!("challenge required")
    };
    f.session
        .ingest_intent_confirmation_prompt(
            approval_fixture::SESSION,
            &HostPromptIngress::trusted_local_hook(
                "UserPromptSubmit",
                prompt.into_bytes(),
                Some(Uuid::new_v4().to_string()),
            ),
        )
        .unwrap()
        .unwrap();
    execute_intent(
        &f.ws,
        f.id,
        IntentOperation::ConfirmBaseline {
            expected_revision: revision,
            requirements,
            reference,
        },
    )
    .unwrap();
    f
}

#[test]
fn only_independently_confirmed_original_mock_policy_can_accept_mock_behavior() {
    for allow_mock in [false, true] {
        let f = confirmed_behavior(allow_mock);
        f.approve("mock-policy", "mock-policy-event");
        ticket_api::storage::store::TicketStore::open(&f.ws)
            .unwrap()
            .close(&f.ticket, "done", Some("fixture"))
            .unwrap();
        let receipt = "ce://default/test/mock-a";
        evidence(&f, "a", receipt);
        let provider = Arc::new(provider_fixture::TestProvider::new());
        let mut record = provider_fixture::put(&f, "a", receipt, &provider);
        record.checks[0].mock = true;
        provider.put(&record);
        let adapter = WorkspaceLifecycle::open(&f.ws).with_test_evidence_provider(provider);
        let result = adapter.complete_waypoint(f.id, "a", &request(&f, receipt));
        if allow_mock {
            assert!(result.is_ok(), "{result:?}");
        } else {
            assert_eq!(result.unwrap_err().code(), "mock_evidence_prohibited");
        }
    }
}
