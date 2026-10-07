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
    AuthorizedDraft, AuthorizedDraftRegistry, AuthorizedDraftRequest,
};
use cofi_storage::authorized_draft::encode_authorized_draft;

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

use cofi_finalization_authorization::{
    AuthorizedFinalization, AuthorizedFinalizationRegistry, AuthorizedFinalizationRequest,
};
use cofi_storage::authorized_finalization::{
    decode_authorized_finalization, encode_authorized_finalization, replay_authorized_finalizations,
};

fn finalization_fixture() -> (
    AuthorizedFinalizationRequest,
    Vec<u8>,
    AuthorizedFinalization,
) {
    let (draft_request, ratings, draft) = p22_fixture();
    let p22 = encode_authorized_draft(&draft_request, &ratings, &draft).unwrap();
    let request = AuthorizedFinalizationRequest::new(
        BillingEventId::new("finalize-event-1").unwrap(),
        draft,
        3 * DAY,
        4 * DAY,
    );
    let expected = AuthorizedFinalizationRegistry::new()
        .finalize(request.clone())
        .unwrap()
        .authorization()
        .clone();
    (request, p22, expected)
}

#[test]
fn authentic_p23_source_result_roundtrips_and_replays() {
    let (request, source, expected) = finalization_fixture();
    let record = encode_authorized_finalization(&request, &source, &expected).unwrap();
    assert_eq!(decode_authorized_finalization(&record).unwrap(), expected);
    let recovered = replay_authorized_finalizations([record.as_slice()]).unwrap();
    assert_eq!(recovered.authorization_count(), 1);
    assert_eq!(recovered.finalization_event_count(), 1);
    assert_eq!(
        recovered.authorization_for_event(request.source_event_id()),
        Some(&expected)
    );
    assert_eq!(
        replay_authorized_finalizations([record.as_slice(), record.as_slice()])
            .unwrap()
            .authorization_count(),
        1
    );
    assert_eq!(
        expected.finalized().to_billing_event().invoice_id(),
        expected.finalized().draft().invoice_id()
    );
}

#[test]
fn wrong_ancestor_event_timestamp_or_total_rejects() {
    let (request, source, expected) = finalization_fixture();
    let record = encode_authorized_finalization(&request, &source, &expected).unwrap();
    let original: serde_json::Value = serde_json::from_slice(&record).unwrap();
    for (path, new_value) in [
        ("/schema_version", serde_json::json!(2)),
        ("/payload/finalized_at_unix_ms", serde_json::json!("1")),
        ("/payload/observed_at_unix_ms", serde_json::json!("0")),
        ("/payload/expected_total_minor", serde_json::json!("999")),
        ("/payload/expected_invoice_id", serde_json::json!("wrong")),
        (
            "/payload/authorized_draft/payload/expected_scope",
            serde_json::json!("wrong"),
        ),
        (
            "/payload/authorized_draft/payload/authorizations",
            serde_json::json!([]),
        ),
    ] {
        let mut edited = original.clone();
        *edited.pointer_mut(path).unwrap() = new_value;
        assert!(
            decode_authorized_finalization(&serde_json::to_vec(&edited).unwrap()).is_err(),
            "{path}"
        );
    }
    let mut extra = original;
    extra["payload"]["injected"] = serde_json::json!(1);
    assert!(decode_authorized_finalization(&serde_json::to_vec(&extra).unwrap()).is_err());
}

#[test]
fn same_invoice_cannot_finalized_twice_or_change_source_id() {
    let (request, source, expected) = finalization_fixture();
    let first = encode_authorized_finalization(&request, &source, &expected).unwrap();
    let other_request = AuthorizedFinalizationRequest::new(
        BillingEventId::new("finalize-event-2").unwrap(),
        request.authorized_draft().clone(),
        3 * DAY,
        4 * DAY,
    );
    let other_accepted = AuthorizedFinalizationRegistry::new()
        .finalize(other_request.clone())
        .unwrap()
        .authorization()
        .clone();
    let second = encode_authorized_finalization(&other_request, &source, &other_accepted).unwrap();
    assert!(replay_authorized_finalizations([first.as_slice(), second.as_slice()]).is_err());
    let mut altered: serde_json::Value = serde_json::from_slice(&first).unwrap();
    altered["payload"]["finalized_at_unix_ms"] = serde_json::json!((3 * DAY + 1).to_string());
    assert!(
        replay_authorized_finalizations([
            first.as_slice(),
            serde_json::to_vec(&altered).unwrap().as_slice()
        ])
        .is_err()
    );
}
