#![allow(clippy::unwrap_used)]

use cofi_disbursements::{
    BeneficiaryReference, DestinationReference, DisbursementCreation, DisbursementEventId,
    DisbursementId, DisbursementSubmission, DisbursementTerminalEvent, ProviderEventReference,
    ProviderRequestReference, ProviderSettlementReference,
};
use cofi_storage::disbursement_creation::encode_disbursement_creation;
use cofi_storage::disbursement_lifecycle::{
    encode_disbursement_submission, encode_disbursement_terminal,
};
use cofi_storage::source_checkpoint::{
    DeclaredSourceFact, DeclaredSourceScope, SupportedSourceKind, compute_untrusted_range,
};

fn creation(event: &str, id: &str, at: i64) -> Vec<u8> {
    encode_disbursement_creation(&DisbursementCreation::new(
        DisbursementEventId::new(event).unwrap(),
        DisbursementId::new(id).unwrap(),
        BeneficiaryReference::new("beneficiary-1").unwrap(),
        DestinationReference::new("destination-1").unwrap(),
        at,
    ))
    .unwrap()
}
fn submission(event: &str, id: &str, provider_ref: &str, at: i64) -> Vec<u8> {
    encode_disbursement_submission(&DisbursementSubmission::new(
        DisbursementEventId::new(event).unwrap(),
        DisbursementId::new(id).unwrap(),
        ProviderRequestReference::new(provider_ref).unwrap(),
        at,
    ))
    .unwrap()
}
fn terminal(event: &str, id: &str, provider_ref: &str, settle_ref: &str, at: i64) -> Vec<u8> {
    encode_disbursement_terminal(&DisbursementTerminalEvent::settled(
        DisbursementEventId::new(event).unwrap(),
        DisbursementId::new(id).unwrap(),
        ProviderEventReference::new(provider_ref).unwrap(),
        ProviderSettlementReference::new(settle_ref).unwrap(),
        at,
    ))
    .unwrap()
}
fn scope() -> DeclaredSourceScope<'static> {
    DeclaredSourceScope {
        authority_id: "caller-source",
        organization_id: "org-a",
        environment_id: "test",
        previous_digest_hex: None,
    }
}
fn fact<'a>(
    seq: &'static str,
    key: &'static str,
    kind: SupportedSourceKind,
    bytes: &'a [u8],
) -> DeclaredSourceFact<'a> {
    DeclaredSourceFact {
        sequence: seq,
        authority_id: "caller-source",
        organization_id: "org-a",
        environment_id: "test",
        source_record_key: key,
        kind,
        bytes,
    }
}

#[test]
fn genesis_disbursement_requires_creation_then_submission_then_terminal() {
    let create = creation("create-1", "disb-1", 10);
    let send = submission("submit-1", "disb-1", "req-1", 20);
    let settle = terminal("terminal-1", "disb-1", "evt-1", "settle-1", 30);
    let c = fact("1", "create-1", SupportedSourceKind::Creation, &create);
    let s = fact("2", "submit-1", SupportedSourceKind::Submission, &send);
    let t = fact("3", "terminal-1", SupportedSourceKind::Terminal, &settle);
    let consistency = compute_untrusted_range(scope(), &[c, s, t]).unwrap();
    assert_eq!(consistency.count(), 3);
    assert!(
        consistency
            .require_independent_source_authentication()
            .is_err()
    );
    let orphan_send = fact("1", "submit-1", SupportedSourceKind::Submission, &send);
    let orphan_terminal = fact("1", "terminal-1", SupportedSourceKind::Terminal, &settle);
    assert!(
        compute_untrusted_range(scope(), &[orphan_send]).is_err(),
        "submission requires prior creation"
    );
    assert!(
        compute_untrusted_range(scope(), &[orphan_terminal]).is_err(),
        "terminal requires prior submission"
    );
    let early_terminal = fact("2", "terminal-1", SupportedSourceKind::Terminal, &settle);
    assert!(
        compute_untrusted_range(scope(), &[c, early_terminal]).is_err(),
        "terminal before submission"
    );
    let later_send = fact("3", "submit-1", SupportedSourceKind::Submission, &send);
    assert!(
        compute_untrusted_range(scope(), &[c, early_terminal, later_send]).is_err(),
        "late submission cannot legitimize terminal"
    );
}

#[test]
fn genesis_disbursement_rejects_cross_flow_duplicates_and_backwards_time() {
    let create = creation("create-1", "disb-1", 10);
    let send = submission("submit-1", "disb-1", "req-1", 20);
    let settle = terminal("terminal-1", "disb-1", "evt-1", "settle-1", 30);
    let c = fact("1", "create-1", SupportedSourceKind::Creation, &create);
    let s = fact("2", "submit-1", SupportedSourceKind::Submission, &send);
    let t = fact("3", "terminal-1", SupportedSourceKind::Terminal, &settle);
    let changed = creation("create-2", "disb-1", 11);
    let duplicate = fact("4", "create-2", SupportedSourceKind::Creation, &changed);
    assert!(
        compute_untrusted_range(scope(), &[c, s, t, duplicate]).is_err(),
        "different source event must not recreate disbursement ID"
    );
    let send_early = submission("submit-1", "disb-1", "req-1", 9);
    let bad = fact(
        "2",
        "submit-1",
        SupportedSourceKind::Submission,
        &send_early,
    );
    assert!(
        compute_untrusted_range(scope(), &[c, bad]).is_err(),
        "submit earlier than creation"
    );
    let terminal_early = terminal("terminal-1", "disb-1", "evt-1", "settle-1", 19);
    let bad = fact(
        "3",
        "terminal-1",
        SupportedSourceKind::Terminal,
        &terminal_early,
    );
    assert!(
        compute_untrusted_range(scope(), &[c, s, bad]).is_err(),
        "terminal earlier than submission"
    );
    let create_two = creation("create-2", "disb-2", 12);
    let send_two = submission("submit-2", "disb-2", "req-2", 23);
    let term_two = terminal("terminal-2", "disb-2", "evt-2", "settle-2", 35);
    let c2 = fact("4", "create-2", SupportedSourceKind::Creation, &create_two);
    let s2 = fact("5", "submit-2", SupportedSourceKind::Submission, &send_two);
    let t2 = fact("6", "terminal-2", SupportedSourceKind::Terminal, &term_two);
    assert!(
        compute_untrusted_range(scope(), &[c, s, t, c2, s2, t2]).is_ok(),
        "independent second disbursement may complete"
    );
    let wrong_send = submission("submit-2", "disb-2", "req-1", 23);
    let bad = fact(
        "5",
        "submit-2",
        SupportedSourceKind::Submission,
        &wrong_send,
    );
    assert!(
        compute_untrusted_range(scope(), &[c, s, t, c2, bad]).is_err(),
        "repeated provider request across disbursements"
    );
    let wrong_terminal = terminal("terminal-2", "disb-2", "evt-1", "settle-2", 35);
    let bad = fact(
        "6",
        "terminal-2",
        SupportedSourceKind::Terminal,
        &wrong_terminal,
    );
    assert!(
        compute_untrusted_range(scope(), &[c, s, t, c2, s2, bad]).is_err(),
        "repeated provider event across disbursements"
    );
    let wrong_settle = terminal("terminal-2", "disb-2", "evt-2", "settle-1", 35);
    let bad = fact(
        "6",
        "terminal-2",
        SupportedSourceKind::Terminal,
        &wrong_settle,
    );
    assert!(
        compute_untrusted_range(scope(), &[c, s, t, c2, s2, bad]).is_err(),
        "repeated settlement reference across disbursements"
    );
    let repeat_send = submission("submit-2", "disb-1", "req-2", 22);
    let bad = fact(
        "4",
        "submit-2",
        SupportedSourceKind::Submission,
        &repeat_send,
    );
    assert!(
        compute_untrusted_range(scope(), &[c, s, t, bad]).is_err(),
        "post-terminal duplicate submission"
    );
    let repeat_term = terminal("terminal-2", "disb-1", "evt-2", "settle-2", 35);
    let bad = fact(
        "4",
        "terminal-2",
        SupportedSourceKind::Terminal,
        &repeat_term,
    );
    assert!(
        compute_untrusted_range(scope(), &[c, s, t, bad]).is_err(),
        "second terminal for the same disbursement"
    );
}

#[test]
fn continuation_without_original_genesis_is_not_trusted() {
    let send = submission("submit-4", "disb-4", "req-4", 100);
    let scope = DeclaredSourceScope {
        previous_digest_hex: Some(
            "0000000000000000000000000000000000000000000000000000000000000000",
        ),
        ..scope()
    };
    let fact = fact("4", "submit-4", SupportedSourceKind::Submission, &send);
    let range = compute_untrusted_range(scope, &[fact]).unwrap();
    assert!(range.require_independent_source_authentication().is_err());
}
