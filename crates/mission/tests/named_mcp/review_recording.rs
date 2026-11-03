use super::*;
use serde_json::Value;

fn fresh() -> (TempDir, std::path::PathBuf, String, Mcp, Value) {
    let (dir, workspace, session_id) = fixture();
    let mut mcp = Mcp::start(&workspace);
    mcp.initialize();
    let intake = call(
        &mut mcp,
        2,
        "mission_intake_create",
        json!({
            "workspace_root":workspace, "session_id":session_id,
            "request_id":"rr-intake", "input":"Plan a revision-bound review."
        }),
    );
    assert_eq!(intake["code"], "ok", "{intake}");
    let created = call(
        &mut mcp,
        3,
        "mission_waypoint_create",
        json!({
            "workspace_root":workspace, "mission_id":intake["mission_id"],
            "request_id":"rr-waypoint", "expected_revision":intake["revision"],
            "waypoint":waypoint("one")
        }),
    );
    assert_eq!(created["code"], "ok", "{created}");
    let read = call(
        &mut mcp,
        4,
        "mission_plan_get",
        json!({
            "workspace_root":workspace, "mission_id":intake["mission_id"]
        }),
    );
    assert_eq!(read["code"], "ok", "{read}");
    let plan = &read["result"]["plan"];
    let args = json!({
        "workspace_root":workspace, "mission_id":intake["mission_id"],
        "request_id":"rr-cold", "expected_revision":read["revision"],
        "kind":"cold_walkthrough", "verdict":"pass", "reviewer":"cold-reviewer",
        "plan_digest":plan["plan_digest"], "evidence_identity":plan["evidence_identity"]
    });
    (dir, workspace, session_id, mcp, args)
}

fn formal(cold: &Value, cold_id: &Value) -> Value {
    let mut args = cold.clone();
    args["request_id"] = json!("rr-formal");
    args["kind"] = json!("formal");
    args["reviewer"] = json!("formal-reviewer");
    args["cold_review_id"] = cold_id.clone();
    args
}

fn read(mcp: &mut Mcp, args: &Value) -> Value {
    call(
        mcp,
        90,
        "mission_plan_get",
        json!({
            "workspace_root":args["workspace_root"], "mission_id":args["mission_id"]
        }),
    )
}

#[test]
fn stdio_fresh_named_reviews_prepare_challenge_without_grant_or_api_seeding() {
    let (_dir, workspace, session_id, mut mcp, args) = fresh();
    let listed = mcp.request(5, "tools/list", json!({}));
    let tool = listed["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "mission_plan_record_review")
        .expect("review recording tool");
    assert_eq!(tool["inputSchema"]["additionalProperties"], false);
    for field in [
        "workspace_root",
        "mission_id",
        "request_id",
        "expected_revision",
        "kind",
        "verdict",
        "reviewer",
        "plan_digest",
        "evidence_identity",
    ] {
        assert!(
            tool["inputSchema"]["required"]
                .as_array()
                .unwrap()
                .contains(&json!(field))
        );
    }
    let cold = call(&mut mcp, 6, "mission_plan_record_review", args.clone());
    assert_eq!(cold["code"], "ok", "{cold}");
    let formal = call(
        &mut mcp,
        7,
        "mission_plan_record_review",
        formal(&args, &cold["result"]["review_id"]),
    );
    assert_eq!(formal["code"], "ok", "{formal}");
    assert_eq!(cold["revision"], args["expected_revision"]);
    assert_eq!(formal["revision"], args["expected_revision"]);
    let prepared = call(
        &mut mcp,
        8,
        "mission_plan_prepare_review",
        json!({
            "workspace_root":workspace, "mission_id":args["mission_id"],
            "session_id":session_id, "request_id":"rr-challenge",
            "expected_revision":args["expected_revision"]
        }),
    );
    assert_eq!(prepared["code"], "ok", "{prepared}");
    assert!(
        prepared["result"]["host_prompt"]
            .as_str()
            .unwrap()
            .starts_with("mission-decision approve ")
    );
    let current = read(&mut mcp, &args);
    assert_eq!(
        current["result"]["reviews"],
        json!([cold["result"], formal["result"]])
    );
    assert_eq!(current["result"]["grants"], json!([]));
    let store = SessionStoreConfig::new(workspace.join(".workflow-tools/session"));
    let authority = mission::approval::MissionApproval::open(&workspace, &store);
    assert_eq!(
        authority
            .check_entry(
                args["mission_id"].as_str().unwrap().parse().unwrap(),
                mission::approval::EntryPath::Dispatch
            )
            .err()
            .unwrap()
            .code(),
        "approval_required"
    );
    let missing = call(
        &mut mcp,
        9,
        "mission_plan_approve",
        json!({
            "workspace_root":workspace, "mission_id":args["mission_id"],
            "request_id":"rr-challenge", "expected_revision":args["expected_revision"],
            "decision":{"session_id":session_id, "challenge_id":"rr-challenge",
                "decision_id":"not-captured", "source_event_id":"not-submitted"}
        }),
    );
    assert_eq!(
        missing["code"], "approval_authority_unavailable",
        "{missing}"
    );
}

#[test]
fn stdio_review_replay_is_nonduplicating_and_divergent_or_stale_retries_fail() {
    let (_dir, _workspace, _session_id, mut mcp, args) = fresh();
    let cold = call(&mut mcp, 5, "mission_plan_record_review", args.clone());
    assert_eq!(cold["code"], "ok", "{cold}");
    let replay = call(&mut mcp, 6, "mission_plan_record_review", args.clone());
    assert_eq!(replay["result"], cold["result"]);
    for (field, value) in [
        ("reviewer", json!("other")),
        ("verdict", json!("fail")),
        ("kind", json!("formal")),
        ("cold_review_id", json!("foreign")),
    ] {
        let mut divergent = args.clone();
        divergent[field] = value;
        let result = call(&mut mcp, 7, "mission_plan_record_review", divergent);
        assert_eq!(result["code"], "approval_conflict", "{field}: {result}");
    }
    let formal_args = formal(&args, &cold["result"]["review_id"]);
    let recorded = call(
        &mut mcp,
        8,
        "mission_plan_record_review",
        formal_args.clone(),
    );
    assert_eq!(recorded["code"], "ok", "{recorded}");
    let replay = call(&mut mcp, 9, "mission_plan_record_review", formal_args);
    assert_eq!(replay["result"], recorded["result"]);
    assert_eq!(
        read(&mut mcp, &args)["result"]["reviews"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let updated = call(
        &mut mcp,
        10,
        "mission_plan_update",
        json!({
            "workspace_root":args["workspace_root"], "mission_id":args["mission_id"],
            "request_id":"rr-edit", "expected_revision":args["expected_revision"],
            "operation":{"op":"set_goal", "title":"Changed goal", "objective":"Invalidate old reviews."}
        }),
    );
    assert_eq!(updated["code"], "ok", "{updated}");
    let current = read(&mut mcp, &args);
    let mut stale_cold = formal(&args, &cold["result"]["review_id"]);
    stale_cold["request_id"] = json!("rr-current-formal-stale-cold");
    stale_cold["expected_revision"] = updated["revision"].clone();
    stale_cold["plan_digest"] = current["result"]["plan"]["plan_digest"].clone();
    stale_cold["evidence_identity"] = current["result"]["plan"]["evidence_identity"].clone();
    let result = call(&mut mcp, 13, "mission_plan_record_review", stale_cold);
    assert_eq!(result["code"], "approval_required", "{result}");
    let stale = call(&mut mcp, 11, "mission_plan_record_review", args.clone());
    assert_eq!(stale["code"], "revision_conflict", "{stale}");
    let mut stale_snapshot = args.clone();
    stale_snapshot["expected_revision"] = updated["revision"].clone();
    let stale = call(&mut mcp, 12, "mission_plan_record_review", stale_snapshot);
    assert_eq!(stale["code"], "approval_stale", "{stale}");
    assert_eq!(
        read(&mut mcp, &args)["result"]["reviews"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn named_review_and_mutation_serialize_on_the_provider_transaction_lock() {
    use std::sync::mpsc;
    use std::time::Duration;
    let (_dir, workspace, _session_id, _mcp, args) = fresh();
    let mission_id = args["mission_id"].as_str().unwrap().parse().unwrap();
    let provider = mission::MissionLifecycle::open(&workspace);
    let held = provider.begin_approval_transaction(mission_id).unwrap();
    let (ready_tx, ready_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let review_args = args.clone();
    let ready = ready_tx.clone();
    let done = done_tx.clone();
    let review = std::thread::spawn(move || {
        ready.send(()).unwrap();
        let outcome = mission::named::call("mission_plan_record_review", review_args);
        done.send(("review", serde_json::to_value(outcome).unwrap()))
            .unwrap();
    });
    let mutation_args = json!({
        "workspace_root":workspace, "mission_id":args["mission_id"],
        "request_id":"rr-concurrent-edit", "expected_revision":args["expected_revision"],
        "operation":{"op":"set_goal","title":"Concurrent edit","objective":"Invalidate reviews."}
    });
    let mutation = std::thread::spawn(move || {
        ready_tx.send(()).unwrap();
        let outcome = mission::named::call("mission_plan_update", mutation_args);
        done_tx
            .send(("edit", serde_json::to_value(outcome).unwrap()))
            .unwrap();
    });
    for _ in 0..2 {
        ready_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    }
    assert!(matches!(
        done_rx.recv_timeout(Duration::from_millis(300)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));
    drop(held);
    for _ in 0..2 {
        let (operation, result) = done_rx.recv_timeout(Duration::from_secs(60)).unwrap();
        if operation == "edit" {
            assert_eq!(result["code"], "ok", "{result}");
        } else if result["code"] == "ok" {
            assert_eq!(result["revision"], args["expected_revision"], "{result}");
            assert_eq!(result["result"]["revision"], args["expected_revision"]);
        } else {
            assert_eq!(result["code"], "revision_conflict", "{result}");
        }
    }
    review.join().unwrap();
    mutation.join().unwrap();
    let store = SessionStoreConfig::new(workspace.join(".workflow-tools/session"));
    let approval = mission::approval::MissionApproval::open(&workspace, &store);
    assert!(approval.grants(mission_id).unwrap().is_empty());
}

#[test]
fn legacy_review_records_deserialize_without_request_identity_and_legacy_calls_append() {
    let f = approval_fixture::linked();
    f.reviewed();
    let reviews = f.approval().reviews(f.id).unwrap();
    for record in reviews {
        assert_eq!(record.request_id, None);
        let json = serde_json::to_value(&record).unwrap();
        assert!(!json.as_object().unwrap().contains_key("request_id"));
        let restored: mission::approval::ReviewRecord = serde_json::from_value(json).unwrap();
        assert_eq!(restored, record);
    }
    let before = f.approval().reviews(f.id).unwrap().len();
    f.reviewed();
    assert_eq!(f.approval().reviews(f.id).unwrap().len(), before + 2);
}

#[test]
fn stdio_invalid_review_evidence_and_ordering_fail_visibly() {
    let (_dir, workspace, session_id, mut mcp, args) = fresh();
    let mut bad_formal = formal(&args, &json!("absent"));
    for cold_id in [Some(json!("absent")), None] {
        if let Some(id) = cold_id {
            bad_formal["cold_review_id"] = id;
        } else {
            bad_formal.as_object_mut().unwrap().remove("cold_review_id");
        }
        let result = call(
            &mut mcp,
            5,
            "mission_plan_record_review",
            bad_formal.clone(),
        );
        assert_eq!(result["code"], "approval_required", "{result}");
    }
    for (field, value, code) in [
        ("request_id", json!("  "), "invalid_request"),
        ("reviewer", json!("  "), "approval_required"),
        ("kind", json!("unknown"), "invalid_request"),
        ("verdict", json!("unknown"), "invalid_request"),
        ("plan_digest", json!("stale"), "approval_stale"),
        ("evidence_identity", json!("stale"), "approval_stale"),
        ("actor", json!("requester"), "invalid_request"),
        ("approve", json!(true), "invalid_request"),
        ("cold_review_id", json!("foreign"), "approval_required"),
    ] {
        let mut invalid = args.clone();
        invalid[field] = value;
        let result = call(&mut mcp, 6, "mission_plan_record_review", invalid);
        assert_eq!(result["code"], code, "{field}: {result}");
        assert!(result["message"].as_str().unwrap().len() > 0);
    }
    let mut failed_args = args.clone();
    failed_args["request_id"] = json!("rr-failing-cold");
    failed_args["verdict"] = json!("fail");
    let failed = call(&mut mcp, 7, "mission_plan_record_review", failed_args);
    assert_eq!(failed["code"], "ok", "{failed}");
    let result = call(
        &mut mcp,
        8,
        "mission_plan_record_review",
        formal(&args, &failed["result"]["review_id"]),
    );
    assert_eq!(result["code"], "approval_required", "{result}");
    let cold = call(&mut mcp, 9, "mission_plan_record_review", args.clone());
    assert_eq!(cold["code"], "ok", "{cold}");
    let mut same = formal(&args, &cold["result"]["review_id"]);
    same["reviewer"] = args["reviewer"].clone();
    let result = call(&mut mcp, 10, "mission_plan_record_review", same);
    assert_eq!(result["code"], "approval_review_not_distinct", "{result}");
    let mut foreign = formal(&args, &json!("review-foreign"));
    foreign["request_id"] = json!("rr-foreign");
    assert_eq!(
        call(&mut mcp, 11, "mission_plan_record_review", foreign)["code"],
        "approval_required"
    );
    let mut failed_formal = formal(&args, &cold["result"]["review_id"]);
    failed_formal["verdict"] = json!("fail");
    assert_eq!(
        call(&mut mcp, 12, "mission_plan_record_review", failed_formal)["code"],
        "ok"
    );
    let prepared = call(
        &mut mcp,
        13,
        "mission_plan_prepare_review",
        json!({
            "workspace_root":workspace, "mission_id":args["mission_id"],
            "session_id":session_id, "request_id":"rr-failed-challenge",
            "expected_revision":args["expected_revision"]
        }),
    );
    assert_eq!(prepared["code"], "approval_required", "{prepared}");
    assert_eq!(read(&mut mcp, &args)["result"]["grants"], json!([]));
}
