#![allow(clippy::unwrap_used)]

use cofi_billing::BillingCustomerId;
use cofi_ledger::{Currency, LedgerScopeId};
use cofi_metering::{MeterId, SubjectId};
use cofi_rating::{RatePlan, RatingPlanId};
use cofi_subscriptions::{
    SubscriptionCreateOutcome, SubscriptionError, SubscriptionEventId, SubscriptionId,
    SubscriptionRegistry, SubscriptionRequest, SubscriptionStatus,
};

const DAY: i64 = 86_400_000;

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
    effective_from_unix_ms: i64,
    effective_until_unix_ms: Option<i64>,
) -> RatePlan {
    RatePlan::new(
        RatingPlanId::new(id).unwrap(),
        meter(meter_id),
        Currency::new("USD").unwrap(),
        25,
        10,
        effective_from_unix_ms,
        effective_until_unix_ms,
    )
    .unwrap()
}

#[allow(clippy::too_many_arguments)]
fn request(
    event: &str,
    subscription: &str,
    scope_id: &str,
    customer_id: &str,
    subject_id: &str,
    rate_plan: RatePlan,
    active_from_unix_ms: i64,
    active_until_unix_ms: Option<i64>,
) -> SubscriptionRequest {
    SubscriptionRequest::new(
        SubscriptionEventId::new(event).unwrap(),
        SubscriptionId::new(subscription).unwrap(),
        scope(scope_id),
        customer(customer_id),
        subject(subject_id),
        rate_plan,
        active_from_unix_ms,
        active_until_unix_ms,
    )
    .unwrap()
}

#[test]
fn subscription_identifiers_fail_closed() {
    assert!(matches!(
        SubscriptionId::new("  "),
        Err(SubscriptionError::EmptyIdentifier("subscription_id"))
    ));
    assert!(matches!(
        SubscriptionEventId::new(""),
        Err(SubscriptionError::EmptyIdentifier("subscription_event_id"))
    ));
}

#[test]
fn subscription_interval_shape_fails_closed() {
    let p = plan("plan-shape", "api", 0, None);
    assert!(matches!(
        SubscriptionRequest::new(
            SubscriptionEventId::new("event-negative").unwrap(),
            SubscriptionId::new("sub-negative").unwrap(),
            scope("org"),
            customer("cust"),
            subject("subject"),
            p.clone(),
            -1,
            None,
        ),
        Err(SubscriptionError::InvalidActiveFrom(-1))
    ));
    assert!(matches!(
        SubscriptionRequest::new(
            SubscriptionEventId::new("event-until").unwrap(),
            SubscriptionId::new("sub-until").unwrap(),
            scope("org"),
            customer("cust"),
            subject("subject"),
            p.clone(),
            0,
            Some(-1),
        ),
        Err(SubscriptionError::InvalidActiveUntil(-1))
    ));
    assert!(matches!(
        SubscriptionRequest::new(
            SubscriptionEventId::new("event-range").unwrap(),
            SubscriptionId::new("sub-range").unwrap(),
            scope("org"),
            customer("cust"),
            subject("subject"),
            p,
            DAY,
            Some(DAY),
        ),
        Err(SubscriptionError::InvalidActiveRange { .. })
    ));
}

#[test]
fn subscription_must_fit_inside_plan_effectiveness() {
    let finite = plan("finite", "api", DAY, Some(4 * DAY));
    assert!(matches!(
        SubscriptionRequest::new(
            SubscriptionEventId::new("before-event").unwrap(),
            SubscriptionId::new("before-sub").unwrap(),
            scope("org"),
            customer("cust"),
            subject("subject"),
            finite.clone(),
            0,
            Some(2 * DAY),
        ),
        Err(SubscriptionError::SubscriptionBeforePlan { .. })
    ));
    assert!(matches!(
        SubscriptionRequest::new(
            SubscriptionEventId::new("open-event").unwrap(),
            SubscriptionId::new("open-sub").unwrap(),
            scope("org"),
            customer("cust"),
            subject("subject"),
            finite.clone(),
            DAY,
            None,
        ),
        Err(SubscriptionError::OpenEndedSubscriptionOnFinitePlan { .. })
    ));
    assert!(matches!(
        SubscriptionRequest::new(
            SubscriptionEventId::new("beyond-event").unwrap(),
            SubscriptionId::new("beyond-sub").unwrap(),
            scope("org"),
            customer("cust"),
            subject("subject"),
            finite,
            DAY,
            Some(5 * DAY),
        ),
        Err(SubscriptionError::SubscriptionBeyondPlan { .. })
    ));
}

#[test]
fn lifecycle_status_is_derived_and_half_open() {
    let request = request(
        "event-status",
        "sub-status",
        "org",
        "cust",
        "subject",
        plan("plan-status", "api", 0, Some(5 * DAY)),
        DAY,
        Some(3 * DAY),
    );
    let mut registry = SubscriptionRegistry::new();
    let subscription = registry.create(request).unwrap().subscription().clone();
    assert_eq!(
        subscription.status_at(0).unwrap(),
        SubscriptionStatus::Scheduled
    );
    assert_eq!(
        subscription.status_at(DAY).unwrap(),
        SubscriptionStatus::Active
    );
    assert_eq!(
        subscription.status_at(3 * DAY - 1).unwrap(),
        SubscriptionStatus::Active
    );
    assert_eq!(
        subscription.status_at(3 * DAY).unwrap(),
        SubscriptionStatus::Ended
    );
    assert!(matches!(
        subscription.status_at(-1),
        Err(SubscriptionError::InvalidStatusTimestamp(-1))
    ));
}

#[test]
fn exact_creation_replay_has_zero_duplicate_effect() {
    let request = request(
        "event-replay",
        "sub-replay",
        "org",
        "cust",
        "subject",
        plan("plan-replay", "api", 0, None),
        0,
        None,
    );
    let mut registry = SubscriptionRegistry::new();
    let first = registry.create(request.clone()).unwrap();
    let replay = registry.create(request).unwrap();
    assert!(matches!(first, SubscriptionCreateOutcome::Created { .. }));
    assert!(matches!(replay, SubscriptionCreateOutcome::Replayed { .. }));
    assert_eq!(first.subscription(), replay.subscription());
    assert_eq!(registry.event_count(), 1);
    assert_eq!(registry.subscription_count(), 1);
}

#[test]
fn event_and_subscription_identity_conflicts_fail_closed() {
    let mut registry = SubscriptionRegistry::new();
    registry
        .create(request(
            "event-conflict",
            "sub-a",
            "org",
            "cust",
            "subject-a",
            plan("plan-a", "api", 0, None),
            0,
            None,
        ))
        .unwrap();

    let event_error = registry
        .create(request(
            "event-conflict",
            "sub-b",
            "org",
            "cust",
            "subject-b",
            plan("plan-b", "api", 0, None),
            0,
            None,
        ))
        .unwrap_err();
    assert!(matches!(
        event_error,
        SubscriptionError::SourceEventConflict(_)
    ));

    let id_error = registry
        .create(request(
            "event-new",
            "sub-a",
            "org",
            "cust",
            "subject-c",
            plan("plan-c", "api", 0, None),
            0,
            None,
        ))
        .unwrap_err();
    assert!(matches!(
        id_error,
        SubscriptionError::SubscriptionIdConflict { .. }
    ));
    assert_eq!(registry.event_count(), 1);
    assert_eq!(registry.subscription_count(), 1);
}

#[test]
fn overlapping_same_commercial_schedule_fails_closed() {
    let mut registry = SubscriptionRegistry::new();
    registry
        .create(request(
            "event-overlap-a",
            "sub-overlap-a",
            "org",
            "cust",
            "subject",
            plan("plan-overlap-a", "api", 0, Some(10 * DAY)),
            DAY,
            Some(4 * DAY),
        ))
        .unwrap();
    let error = registry
        .create(request(
            "event-overlap-b",
            "sub-overlap-b",
            "org",
            "cust",
            "subject",
            plan("plan-overlap-b", "api", 0, Some(10 * DAY)),
            3 * DAY,
            Some(6 * DAY),
        ))
        .unwrap_err();
    assert!(matches!(
        error,
        SubscriptionError::OverlappingSchedule { .. }
    ));
    assert_eq!(registry.event_count(), 1);
    assert_eq!(registry.subscription_count(), 1);
}

#[test]
fn adjacent_same_key_schedules_are_allowed() {
    let mut registry = SubscriptionRegistry::new();
    registry
        .create(request(
            "event-adjacent-a",
            "sub-adjacent-a",
            "org",
            "cust",
            "subject",
            plan("plan-adjacent-a", "api", 0, Some(10 * DAY)),
            DAY,
            Some(3 * DAY),
        ))
        .unwrap();
    registry
        .create(request(
            "event-adjacent-b",
            "sub-adjacent-b",
            "org",
            "cust",
            "subject",
            plan("plan-adjacent-b", "api", 0, Some(10 * DAY)),
            3 * DAY,
            Some(5 * DAY),
        ))
        .unwrap();
    assert_eq!(registry.subscription_count(), 2);
    let resolved = registry
        .resolve_for_window(
            &scope("org"),
            &customer("cust"),
            &subject("subject"),
            &meter("api"),
            3 * DAY,
            4 * DAY,
        )
        .unwrap()
        .unwrap();
    assert_eq!(resolved.id().as_str(), "sub-adjacent-b");
}

#[test]
fn overlapping_time_is_allowed_for_different_commercial_keys() {
    let mut registry = SubscriptionRegistry::new();
    for (event, id, org, cust, subj, meter_id) in [
        (
            "event-key-a",
            "sub-key-a",
            "org-a",
            "cust",
            "subject",
            "api",
        ),
        (
            "event-key-b",
            "sub-key-b",
            "org-b",
            "cust",
            "subject",
            "api",
        ),
        (
            "event-key-c",
            "sub-key-c",
            "org-a",
            "cust-2",
            "subject",
            "api",
        ),
        (
            "event-key-d",
            "sub-key-d",
            "org-a",
            "cust",
            "subject-2",
            "api",
        ),
        (
            "event-key-e",
            "sub-key-e",
            "org-a",
            "cust",
            "subject",
            "gpu",
        ),
    ] {
        registry
            .create(request(
                event,
                id,
                org,
                cust,
                subj,
                plan(&format!("plan-{id}"), meter_id, 0, None),
                DAY,
                None,
            ))
            .unwrap();
    }
    assert_eq!(registry.subscription_count(), 5);
}

#[test]
fn rejected_overlap_reserves_nothing_and_corrected_retry_succeeds() {
    let mut registry = SubscriptionRegistry::new();
    registry
        .create(request(
            "event-existing",
            "sub-existing",
            "org",
            "cust",
            "subject",
            plan("plan-existing", "api", 0, Some(10 * DAY)),
            DAY,
            Some(3 * DAY),
        ))
        .unwrap();

    let overlapping = request(
        "event-retry",
        "sub-retry",
        "org",
        "cust",
        "subject",
        plan("plan-retry", "api", 0, Some(10 * DAY)),
        2 * DAY,
        Some(4 * DAY),
    );
    assert!(matches!(
        registry.create(overlapping),
        Err(SubscriptionError::OverlappingSchedule { .. })
    ));
    assert_eq!(registry.event_count(), 1);
    assert_eq!(registry.subscription_count(), 1);

    let corrected = request(
        "event-retry",
        "sub-retry",
        "org",
        "cust",
        "subject",
        plan("plan-retry", "api", 0, Some(10 * DAY)),
        3 * DAY,
        Some(4 * DAY),
    );
    assert!(matches!(
        registry.create(corrected).unwrap(),
        SubscriptionCreateOutcome::Created { .. }
    ));
    assert_eq!(registry.event_count(), 2);
    assert_eq!(registry.subscription_count(), 2);
}

#[test]
fn exact_window_resolution_returns_immutable_plan_snapshot() {
    let expected_plan = plan("plan-resolve", "api", 0, Some(10 * DAY));
    let mut registry = SubscriptionRegistry::new();
    registry
        .create(request(
            "event-resolve",
            "sub-resolve",
            "org",
            "cust",
            "subject",
            expected_plan.clone(),
            DAY,
            Some(5 * DAY),
        ))
        .unwrap();

    let resolved = registry
        .resolve_for_window(
            &scope("org"),
            &customer("cust"),
            &subject("subject"),
            &meter("api"),
            2 * DAY,
            4 * DAY,
        )
        .unwrap()
        .unwrap();
    assert_eq!(resolved.plan(), &expected_plan);
    assert_eq!(resolved.billing_customer_id().as_str(), "cust");
    assert_eq!(resolved.subject_id().as_str(), "subject");
    assert_eq!(resolved.organization_scope().as_str(), "org");
}

#[test]
fn wrong_binding_gap_and_partial_window_return_no_assignment() {
    let mut registry = SubscriptionRegistry::new();
    registry
        .create(request(
            "event-none",
            "sub-none",
            "org",
            "cust",
            "subject",
            plan("plan-none", "api", 0, Some(10 * DAY)),
            2 * DAY,
            Some(4 * DAY),
        ))
        .unwrap();

    for result in [
        registry
            .resolve_for_window(
                &scope("other"),
                &customer("cust"),
                &subject("subject"),
                &meter("api"),
                2 * DAY,
                3 * DAY,
            )
            .unwrap(),
        registry
            .resolve_for_window(
                &scope("org"),
                &customer("other"),
                &subject("subject"),
                &meter("api"),
                2 * DAY,
                3 * DAY,
            )
            .unwrap(),
        registry
            .resolve_for_window(
                &scope("org"),
                &customer("cust"),
                &subject("other"),
                &meter("api"),
                2 * DAY,
                3 * DAY,
            )
            .unwrap(),
        registry
            .resolve_for_window(
                &scope("org"),
                &customer("cust"),
                &subject("subject"),
                &meter("gpu"),
                2 * DAY,
                3 * DAY,
            )
            .unwrap(),
        registry
            .resolve_for_window(
                &scope("org"),
                &customer("cust"),
                &subject("subject"),
                &meter("api"),
                DAY,
                3 * DAY,
            )
            .unwrap(),
        registry
            .resolve_for_window(
                &scope("org"),
                &customer("cust"),
                &subject("subject"),
                &meter("api"),
                3 * DAY,
                5 * DAY,
            )
            .unwrap(),
    ] {
        assert!(result.is_none());
    }
}

#[test]
fn resolution_window_validation_fails_closed() {
    let registry = SubscriptionRegistry::new();
    assert!(matches!(
        registry.resolve_for_window(
            &scope("org"),
            &customer("cust"),
            &subject("subject"),
            &meter("api"),
            -1,
            DAY,
        ),
        Err(SubscriptionError::InvalidWindow { .. })
    ));
    assert!(matches!(
        registry.resolve_for_window(
            &scope("org"),
            &customer("cust"),
            &subject("subject"),
            &meter("api"),
            DAY,
            DAY,
        ),
        Err(SubscriptionError::InvalidWindow { .. })
    ));
}
