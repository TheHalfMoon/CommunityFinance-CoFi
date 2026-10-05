#![allow(clippy::unwrap_used)]

use cofi_billing::{BillingCustomerId, BillingEventId, BillingInvoiceId};
use cofi_invoice_authorization::{
    AuthorizedDraftError, AuthorizedDraftOutcome, AuthorizedDraftRegistry, AuthorizedDraftRequest,
};
use cofi_invoicing::InvoicingError;
use cofi_ledger::{Currency, LedgerScopeId};
use cofi_metering::{
    Aggregation, EventType, MeterDefinition, MeterId, MeteringEngine, SubjectId, UsageAggregate,
    UsageEvent, UsageEventId, UsageValue, WindowSize,
};
use cofi_rating::{RatePlan, RatingEventId, RatingPlanId};
use cofi_rating_authorization::{
    AuthorizedRating, AuthorizedRatingRegistry, AuthorizedRatingRequest,
};
use cofi_subscriptions::{
    SubscriptionEventId, SubscriptionId, SubscriptionRegistry, SubscriptionRequest,
};

const DAY: i64 = 86_400_000;

fn usd() -> Currency {
    Currency::new("USD").unwrap()
}

fn scope(value: &str) -> LedgerScopeId {
    LedgerScopeId::new(value).unwrap()
}

fn customer(value: &str) -> BillingCustomerId {
    BillingCustomerId::new(value).unwrap()
}

fn subject(value: &str) -> SubjectId {
    SubjectId::new(value).unwrap()
}

fn meter(value: &str) -> MeterId {
    MeterId::new(value).unwrap()
}

fn plan(
    id: &str,
    meter_id: &str,
    price: i128,
    included: i128,
    effective_from: i64,
    effective_until: Option<i64>,
) -> RatePlan {
    RatePlan::new(
        RatingPlanId::new(id).unwrap(),
        meter(meter_id),
        usd(),
        price,
        included,
        effective_from,
        effective_until,
    )
    .unwrap()
}

fn count_aggregate(
    meter_id: &str,
    subject_id: &str,
    window_start: i64,
    count: u64,
) -> UsageAggregate {
    let meter_id = meter(meter_id);
    let subject_id = subject(subject_id);
    let event_type = EventType::new("api.call").unwrap();
    let mut engine = MeteringEngine::new();
    engine
        .register_meter(
            MeterDefinition::new(
                meter_id.clone(),
                event_type.clone(),
                Aggregation::Count,
                WindowSize::Day,
                Some(0),
            )
            .unwrap(),
        )
        .unwrap();

    for index in 0..count {
        engine
            .ingest(
                UsageEvent::new(
                    UsageEventId::new(format!(
                        "usage-{meter_id}-{subject_id}-{window_start}-{index}",
                        meter_id = meter_id.as_str(),
                        subject_id = subject_id.as_str()
                    ))
                    .unwrap(),
                    meter_id.clone(),
                    event_type.clone(),
                    subject_id.clone(),
                    window_start + 1_000 + i64::try_from(index).unwrap(),
                    window_start + 2_000 + i64::try_from(index).unwrap(),
                    UsageValue::Count,
                )
                .unwrap(),
            )
            .unwrap();
    }

    engine
        .aggregate(&meter_id, &subject_id, window_start, window_start + DAY)
        .unwrap()
}

#[allow(clippy::too_many_arguments)]
fn add_subscription(
    registry: &mut SubscriptionRegistry,
    event_id: &str,
    subscription_id: &str,
    scope_id: &str,
    customer_id: &str,
    subject_id: &str,
    plan: RatePlan,
    active_from: i64,
    active_until: Option<i64>,
) {
    registry
        .create(
            SubscriptionRequest::new(
                SubscriptionEventId::new(event_id).unwrap(),
                SubscriptionId::new(subscription_id).unwrap(),
                scope(scope_id),
                customer(customer_id),
                subject(subject_id),
                plan,
                active_from,
                active_until,
            )
            .unwrap(),
        )
        .unwrap();
}

fn authorize(
    subscriptions: &SubscriptionRegistry,
    event_id: &str,
    scope_id: &str,
    customer_id: &str,
    aggregate: UsageAggregate,
    rated_at: i64,
) -> AuthorizedRating {
    let mut ratings = AuthorizedRatingRegistry::new();
    ratings
        .rate(
            AuthorizedRatingRequest::new(
                RatingEventId::new(event_id).unwrap(),
                scope(scope_id),
                customer(customer_id),
                aggregate,
                rated_at,
            ),
            subscriptions,
        )
        .unwrap()
        .authorization()
        .clone()
}

#[allow(clippy::too_many_arguments)]
fn one_authorization(
    rating_event: &str,
    subscription_event: &str,
    subscription_id: &str,
    plan_id: &str,
    scope_id: &str,
    customer_id: &str,
    subject_id: &str,
    meter_id: &str,
    window_start: i64,
    usage_count: u64,
    included: i128,
) -> AuthorizedRating {
    let mut subscriptions = SubscriptionRegistry::new();
    add_subscription(
        &mut subscriptions,
        subscription_event,
        subscription_id,
        scope_id,
        customer_id,
        subject_id,
        plan(
            plan_id,
            meter_id,
            10,
            included,
            window_start,
            Some(window_start + DAY),
        ),
        window_start,
        Some(window_start + DAY),
    );
    authorize(
        &subscriptions,
        rating_event,
        scope_id,
        customer_id,
        count_aggregate(meter_id, subject_id, window_start, usage_count),
        window_start + DAY,
    )
}

fn request(
    event_id: &str,
    invoice_id: &str,
    period_start: i64,
    period_end: i64,
    observed_at: i64,
    authorizations: Vec<AuthorizedRating>,
) -> Result<AuthorizedDraftRequest, AuthorizedDraftError> {
    AuthorizedDraftRequest::new(
        BillingEventId::new(event_id).unwrap(),
        BillingInvoiceId::new(invoice_id).unwrap(),
        period_start,
        period_end,
        observed_at,
        authorizations,
    )
}

fn standard_authorization(event_id: &str) -> AuthorizedRating {
    one_authorization(
        event_id,
        &format!("subscription-event-{event_id}"),
        &format!("subscription-{event_id}"),
        &format!("plan-{event_id}"),
        "org-1",
        "customer-1",
        &format!("subject-{event_id}"),
        &format!("meter-{event_id}"),
        0,
        3,
        1,
    )
}

#[test]
fn empty_authorization_set_fails_closed() {
    assert_eq!(
        request("draft-empty", "invoice-empty", 0, DAY, DAY, vec![]),
        Err(AuthorizedDraftError::EmptyAuthorizationSet)
    );
}

#[test]
fn successful_assembly_derives_scope_customer_and_preserves_lineage() {
    let authorization = standard_authorization("rating-success");
    let mut registry = AuthorizedDraftRegistry::new();
    let outcome = registry
        .assemble(
            request(
                "draft-success",
                "invoice-success",
                0,
                DAY,
                DAY,
                vec![authorization.clone()],
            )
            .unwrap(),
        )
        .unwrap();

    assert!(matches!(outcome, AuthorizedDraftOutcome::Created { .. }));
    let authorized_draft = outcome.authorization();
    assert_eq!(
        authorized_draft.draft().organization_scope().as_str(),
        "org-1"
    );
    assert_eq!(
        authorized_draft.draft().customer_id().as_str(),
        "customer-1"
    );
    assert_eq!(
        authorized_draft.authorizations(),
        std::slice::from_ref(&authorization)
    );
    assert_eq!(
        authorized_draft.authorizations()[0]
            .subscription()
            .id()
            .as_str(),
        "subscription-rating-success"
    );
    assert_eq!(
        authorized_draft.authorizations()[0]
            .subscription()
            .source_event_id()
            .as_str(),
        "subscription-event-rating-success"
    );
    assert_eq!(
        authorized_draft.draft().lines()[0].rated_charge(),
        authorization.charge()
    );
    assert_eq!(registry.authorization_count(), 1);
    assert_eq!(registry.draft_event_count(), 1);
}

#[test]
fn input_order_is_canonical_and_exact_replay_is_idempotent() {
    let authorization_b = standard_authorization("rating-b");
    let authorization_a = standard_authorization("rating-a");
    let first_request = request(
        "draft-replay",
        "invoice-replay",
        0,
        DAY,
        DAY,
        vec![authorization_b.clone(), authorization_a.clone()],
    )
    .unwrap();
    let replay_request = request(
        "draft-replay",
        "invoice-replay",
        0,
        DAY,
        DAY,
        vec![authorization_a, authorization_b],
    )
    .unwrap();
    assert_eq!(first_request, replay_request);

    let mut registry = AuthorizedDraftRegistry::new();
    let first = registry.assemble(first_request).unwrap();
    let replay = registry.assemble(replay_request).unwrap();
    assert!(matches!(first, AuthorizedDraftOutcome::Created { .. }));
    assert!(matches!(replay, AuthorizedDraftOutcome::Replayed { .. }));
    assert_eq!(first.authorization(), replay.authorization());
    assert_eq!(registry.authorization_count(), 1);
    assert_eq!(registry.draft_event_count(), 1);
}

#[test]
fn mixed_organization_scope_fails_before_draft_mutation() {
    let first = standard_authorization("rating-scope-a");
    let second = one_authorization(
        "rating-scope-b",
        "subscription-event-scope-b",
        "subscription-scope-b",
        "plan-scope-b",
        "org-2",
        "customer-1",
        "subject-scope-b",
        "meter-scope-b",
        0,
        1,
        0,
    );
    assert_eq!(
        request(
            "draft-mixed-scope",
            "invoice-mixed-scope",
            0,
            DAY,
            DAY,
            vec![first, second]
        ),
        Err(AuthorizedDraftError::MixedOrganizationScope)
    );
    let registry = AuthorizedDraftRegistry::new();
    assert_eq!(registry.authorization_count(), 0);
    assert_eq!(registry.draft_event_count(), 0);
}

#[test]
fn mixed_billing_customer_fails_before_draft_mutation() {
    let first = standard_authorization("rating-customer-a");
    let second = one_authorization(
        "rating-customer-b",
        "subscription-event-customer-b",
        "subscription-customer-b",
        "plan-customer-b",
        "org-1",
        "customer-2",
        "subject-customer-b",
        "meter-customer-b",
        0,
        1,
        0,
    );
    assert_eq!(
        request(
            "draft-mixed-customer",
            "invoice-mixed-customer",
            0,
            DAY,
            DAY,
            vec![first, second]
        ),
        Err(AuthorizedDraftError::MixedBillingCustomer)
    );
    let registry = AuthorizedDraftRegistry::new();
    assert_eq!(registry.authorization_count(), 0);
    assert_eq!(registry.draft_event_count(), 0);
}

#[test]
fn adjacent_subscription_plan_transitions_can_share_one_draft() {
    let mut subscriptions = SubscriptionRegistry::new();
    add_subscription(
        &mut subscriptions,
        "subscription-event-day-1",
        "subscription-day-1",
        "org-1",
        "customer-1",
        "subject-shared",
        plan("plan-day-1", "meter-shared", 10, 0, 0, Some(DAY)),
        0,
        Some(DAY),
    );
    add_subscription(
        &mut subscriptions,
        "subscription-event-day-2",
        "subscription-day-2",
        "org-1",
        "customer-1",
        "subject-shared",
        plan("plan-day-2", "meter-shared", 20, 0, DAY, Some(2 * DAY)),
        DAY,
        Some(2 * DAY),
    );
    let first = authorize(
        &subscriptions,
        "rating-day-1",
        "org-1",
        "customer-1",
        count_aggregate("meter-shared", "subject-shared", 0, 1),
        DAY,
    );
    let second = authorize(
        &subscriptions,
        "rating-day-2",
        "org-1",
        "customer-1",
        count_aggregate("meter-shared", "subject-shared", DAY, 1),
        2 * DAY,
    );

    let mut registry = AuthorizedDraftRegistry::new();
    let outcome = registry
        .assemble(
            request(
                "draft-transition",
                "invoice-transition",
                0,
                2 * DAY,
                2 * DAY,
                vec![second, first],
            )
            .unwrap(),
        )
        .unwrap();
    let subscriptions = outcome
        .authorization()
        .authorizations()
        .iter()
        .map(|authorization| authorization.subscription().id().as_str())
        .collect::<Vec<_>>();
    assert_eq!(subscriptions.len(), 2);
    assert!(subscriptions.contains(&"subscription-day-1"));
    assert!(subscriptions.contains(&"subscription-day-2"));
    assert_eq!(outcome.authorization().draft().lines().len(), 2);
    assert_eq!(outcome.authorization().draft().total_minor(), 30);
}

#[test]
fn p18_validation_error_reserves_nothing_and_corrected_retry_succeeds() {
    let authorization = standard_authorization("rating-retry");
    let mut registry = AuthorizedDraftRegistry::new();
    let bad = request(
        "draft-retry",
        "invoice-retry",
        DAY,
        DAY,
        DAY,
        vec![authorization.clone()],
    )
    .unwrap();
    assert_eq!(
        registry.assemble(bad),
        Err(AuthorizedDraftError::Invoicing(
            InvoicingError::InvalidBillingPeriod {
                period_start_unix_ms: DAY,
                period_end_unix_ms: DAY,
            }
        ))
    );
    assert_eq!(registry.authorization_count(), 0);
    assert_eq!(registry.draft_event_count(), 0);

    let corrected = registry
        .assemble(
            request(
                "draft-retry",
                "invoice-retry",
                0,
                DAY,
                DAY,
                vec![authorization],
            )
            .unwrap(),
        )
        .unwrap();
    assert!(matches!(corrected, AuthorizedDraftOutcome::Created { .. }));
    assert_eq!(registry.authorization_count(), 1);
    assert_eq!(registry.draft_event_count(), 1);
}

#[test]
fn conflicting_source_event_fails_closed_before_second_draft() {
    let first = standard_authorization("rating-conflict-a");
    let second = standard_authorization("rating-conflict-b");
    let mut registry = AuthorizedDraftRegistry::new();
    registry
        .assemble(
            request(
                "draft-conflict",
                "invoice-conflict-a",
                0,
                DAY,
                DAY,
                vec![first],
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(
        registry.assemble(
            request(
                "draft-conflict",
                "invoice-conflict-b",
                0,
                DAY,
                DAY,
                vec![second],
            )
            .unwrap()
        ),
        Err(AuthorizedDraftError::SourceEventConflict(
            BillingEventId::new("draft-conflict").unwrap()
        ))
    );
    assert_eq!(registry.authorization_count(), 1);
    assert_eq!(registry.draft_event_count(), 1);
}

#[test]
fn canonical_p18_charge_binding_blocks_second_invoice_history() {
    let authorization = standard_authorization("rating-shared");
    let mut registry = AuthorizedDraftRegistry::new();
    registry
        .assemble(
            request(
                "draft-first",
                "invoice-first",
                0,
                DAY,
                DAY,
                vec![authorization.clone()],
            )
            .unwrap(),
        )
        .unwrap();

    let error = registry
        .assemble(
            request(
                "draft-second",
                "invoice-second",
                0,
                DAY,
                DAY,
                vec![authorization.clone()],
            )
            .unwrap(),
        )
        .unwrap_err();
    assert!(matches!(
        error,
        AuthorizedDraftError::Invoicing(InvoicingError::RatedChargeAlreadyBound {
            rated_charge_id,
            existing_invoice_id,
        }) if rated_charge_id == *authorization.charge().id()
            && existing_invoice_id == BillingInvoiceId::new("invoice-first").unwrap()
    ));
    assert_eq!(registry.authorization_count(), 1);
    assert_eq!(registry.draft_event_count(), 1);
}

#[test]
fn duplicate_authorization_delegates_duplicate_charge_semantics_to_p18() {
    let authorization = standard_authorization("rating-duplicate");
    let mut registry = AuthorizedDraftRegistry::new();
    let error = registry
        .assemble(
            request(
                "draft-duplicate",
                "invoice-duplicate",
                0,
                DAY,
                DAY,
                vec![authorization.clone(), authorization.clone()],
            )
            .unwrap(),
        )
        .unwrap_err();
    assert_eq!(
        error,
        AuthorizedDraftError::Invoicing(InvoicingError::DuplicateRatedChargeId(
            authorization.charge().id().clone()
        ))
    );
    assert_eq!(registry.authorization_count(), 0);
    assert_eq!(registry.draft_event_count(), 0);
}

#[test]
fn zero_valued_authorization_remains_an_explicit_draft_line() {
    let authorization = one_authorization(
        "rating-zero",
        "subscription-event-zero",
        "subscription-zero",
        "plan-zero",
        "org-1",
        "customer-1",
        "subject-zero",
        "meter-zero",
        0,
        2,
        2,
    );
    assert_eq!(authorization.charge().amount_minor(), 0);

    let mut registry = AuthorizedDraftRegistry::new();
    let outcome = registry
        .assemble(
            request(
                "draft-zero",
                "invoice-zero",
                0,
                DAY,
                DAY,
                vec![authorization],
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(outcome.authorization().draft().lines().len(), 1);
    assert_eq!(outcome.authorization().draft().lines()[0].amount_minor(), 0);
    assert_eq!(outcome.authorization().draft().total_minor(), 0);
}
