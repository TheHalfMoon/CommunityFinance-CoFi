#![allow(clippy::unwrap_used)]

use cofi_billing::{BillingCustomerId, BillingEventId, BillingInvoiceId};
use cofi_ledger::{Currency, LedgerScopeId};
use cofi_metering::{
    Aggregation, EventType, MeterDefinition, MeterId, MeteringEngine, SubjectId, UsageEvent,
    UsageEventId, UsageValue, WindowSize,
};
use cofi_rating::{RatePlan, RatingEventId, RatingPlanId};
use cofi_rating_authorization::{
    AuthorizedRating, AuthorizedRatingRegistry, AuthorizedRatingRequest,
};
use cofi_storage::CodecError;
use cofi_storage::authorized_rating::encode_authorized_rating;
use cofi_storage::rating::{RatingAcceptance, encode_rating_acceptance};
use cofi_subscriptions::{
    SubscriptionEventId, SubscriptionId, SubscriptionRegistry, SubscriptionRequest,
};

const DAY: i64 = 86_400_000;

struct Fixture {
    subscription: SubscriptionRequest,
    evidence: Vec<u8>,
    accepted: AuthorizedRating,
}

fn fixture() -> Fixture {
    let meter = MeterDefinition::new(
        MeterId::new("api").unwrap(),
        EventType::new("call").unwrap(),
        Aggregation::Count,
        WindowSize::Day,
        Some(0),
    )
    .unwrap();
    let events = (0..3)
        .map(|idx| {
            UsageEvent::new(
                UsageEventId::new(format!("source-{idx}")).unwrap(),
                meter.id().clone(),
                EventType::new("call").unwrap(),
                SubjectId::new("subject-1").unwrap(),
                1_000 + idx,
                2_000 + idx,
                UsageValue::Count,
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    let plan = RatePlan::new(
        RatingPlanId::new("rate-1").unwrap(),
        meter.id().clone(),
        Currency::new("SAR").unwrap(),
        10,
        1,
        0,
        Some(2 * DAY),
    )
    .unwrap();
    let subscription = SubscriptionRequest::new(
        SubscriptionEventId::new("sub-event").unwrap(),
        SubscriptionId::new("subscription-1").unwrap(),
        LedgerScopeId::new("org-1").unwrap(),
        BillingCustomerId::new("customer-1").unwrap(),
        SubjectId::new("subject-1").unwrap(),
        plan.clone(),
        0,
        Some(DAY),
    )
    .unwrap();
    let mut subscriptions = SubscriptionRegistry::new();
    subscriptions.create(subscription.clone()).unwrap();
    let mut meter_engine = MeteringEngine::new();
    meter_engine.register_meter(meter.clone()).unwrap();
    for ev in &events {
        meter_engine.ingest(ev.clone()).unwrap();
    }
    let aggregate = meter_engine
        .aggregate(meter.id(), &SubjectId::new("subject-1").unwrap(), 0, DAY)
        .unwrap();
    let rating_id = RatingEventId::new("rating-1").unwrap();
    let customer = BillingCustomerId::new("customer-1").unwrap();
    let outcome = AuthorizedRatingRegistry::new()
        .rate(
            AuthorizedRatingRequest::new(
                rating_id.clone(),
                LedgerScopeId::new("org-1").unwrap(),
                customer.clone(),
                aggregate,
                DAY,
            ),
            &subscriptions,
        )
        .unwrap();
    let accepted = outcome.authorization().clone();
    let evidence = encode_rating_acceptance(&RatingAcceptance {
        meter: &meter,
        events: &events,
        plan: &plan,
        rating_event_id: &rating_id,
        subject_id: &SubjectId::new("subject-1").unwrap(),
        billing_customer_id: &customer,
        window_start_unix_ms: 0,
        window_end_unix_ms: DAY,
        rated_at_unix_ms: DAY,
        accepted_charge: accepted.charge(),
    })
    .unwrap();
    Fixture {
        subscription,
        evidence,
        accepted,
    }
}

use cofi_invoice_authorization::{
    AuthorizedDraft, AuthorizedDraftOutcome, AuthorizedDraftRegistry, AuthorizedDraftRequest,
};
use cofi_storage::authorized_draft::{
    decode_authorized_draft, encode_authorized_draft, replay_authorized_drafts,
};

fn p22_fixture() -> (AuthorizedDraftRequest, Vec<Vec<u8>>, AuthorizedDraft) {
    let f = fixture();
    let p21 = encode_authorized_rating(&f.subscription, &f.evidence, &f.accepted).unwrap();
    let request = AuthorizedDraftRequest::new(
        BillingEventId::new("draft-event-1").unwrap(),
        BillingInvoiceId::new("invoice-1").unwrap(),
        0,
        DAY,
        2 * DAY,
        vec![f.accepted],
    )
    .unwrap();
    let expected = AuthorizedDraftRegistry::new()
        .assemble(request.clone())
        .unwrap()
        .authorization()
        .clone();
    (request, vec![p21], expected)
}

#[test]
fn accepted_authorized_draft_has_exact_original_lineage_and_replay() {
    let (request, receipts, accepted) = p22_fixture();
    let bytes = encode_authorized_draft(&request, &receipts, &accepted).unwrap();
    assert_eq!(decode_authorized_draft(&bytes).unwrap(), accepted);
    let mut recovered = replay_authorized_drafts([bytes.as_slice()]).unwrap();
    assert_eq!(recovered.authorization_count(), 1);
    assert_eq!(recovered.draft_event_count(), 1);
    assert_eq!(
        recovered.authorization_for_event(request.event_id()),
        Some(&accepted)
    );
    assert!(matches!(
        recovered.assemble(request),
        Ok(AuthorizedDraftOutcome::Replayed { .. })
    ));
    assert_eq!(
        replay_authorized_drafts([bytes.as_slice(), bytes.as_slice()])
            .unwrap()
            .authorization_count(),
        1
    );
}

#[test]
fn changed_ancestry_total_event_identity_and_scope_rejected() {
    let (request, receipts, accepted) = p22_fixture();
    let original = encode_authorized_draft(&request, &receipts, &accepted).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&original).unwrap();
    for (path, replacement) in [
        ("/schema_version", serde_json::json!(2)),
        ("/payload/invoice_id", serde_json::json!("another-invoice")),
        ("/payload/expected_total_minor", serde_json::json!("999")),
        ("/payload/expected_scope", serde_json::json!("another-org")),
        (
            "/payload/authorizations/0/payload/rating/payload/usage_events/0/payload/id",
            serde_json::json!("fake-event"),
        ),
        (
            "/payload/authorizations/0/payload/subscription/payload/plan/payload/unit_price_minor",
            serde_json::json!("55"),
        ),
        ("/payload/authorizations", serde_json::json!([])),
    ] {
        let mut changed = value.clone();
        *changed.pointer_mut(path).unwrap() = replacement;
        assert!(
            decode_authorized_draft(&serde_json::to_vec(&changed).unwrap()).is_err(),
            "{path}"
        );
    }
    let mut changed = value.clone();
    changed["payload"]["new_field"] = serde_json::json!(true);
    assert!(decode_authorized_draft(&serde_json::to_vec(&changed).unwrap()).is_err());
}

#[test]
fn cannot_bind_same_original_charge_to_two_drafts_or_swap_under_same_event() {
    let (req, receipts, accepted) = p22_fixture();
    let first = encode_authorized_draft(&req, &receipts, &accepted).unwrap();
    let second_req = AuthorizedDraftRequest::new(
        BillingEventId::new("draft-event-2").unwrap(),
        BillingInvoiceId::new("invoice-2").unwrap(),
        0,
        DAY,
        2 * DAY,
        req.authorizations().to_vec(),
    )
    .unwrap();
    let second_expected = AuthorizedDraftRegistry::new()
        .assemble(second_req.clone())
        .unwrap()
        .authorization()
        .clone();
    let second = encode_authorized_draft(&second_req, &receipts, &second_expected).unwrap();
    assert!(replay_authorized_drafts([first.as_slice(), second.as_slice()]).is_err());

    let mut changed: serde_json::Value = serde_json::from_slice(&first).unwrap();
    changed["payload"]["invoice_id"] = serde_json::json!("other");
    assert!(
        replay_authorized_drafts([
            first.as_slice(),
            serde_json::to_vec(&changed).unwrap().as_slice()
        ])
        .is_err()
    );
}

#[test]
fn p22_rejects_oversize_outer_fact_and_independent_p21_receipt() {
    let (request, source, accepted) = p22_fixture();
    let mut encoded = encode_authorized_draft(&request, &source, &accepted).unwrap();
    encoded.resize(1024 * 1024, b' ');
    assert_eq!(decode_authorized_draft(&encoded).unwrap(), accepted);
    encoded.push(b' ');
    assert!(matches!(
        decode_authorized_draft(&encoded),
        Err(CodecError::InvalidPayload(_))
    ));
    let mut oversized = source[0].clone();
    oversized.resize(1024 * 1024 + 1, b' ');
    assert!(matches!(
        encode_authorized_draft(&request, &[oversized], &accepted),
        Err(CodecError::InvalidPayload(_))
    ));
}
