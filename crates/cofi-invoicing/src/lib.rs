use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

use cofi_billing::{
    BillingCustomerId, BillingEventId, BillingInvoiceId, InvoiceEvent, InvoiceStatus,
};
use cofi_ledger::{Currency, LedgerScopeId};
use cofi_rating::{RatedCharge, RatedChargeId};

mod finalization;
pub use finalization::{
    FinalizationError, FinalizationOutcome, FinalizationRegistry, FinalizationRequest,
    FinalizedInvoice,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftInvoiceLine {
    rated_charge: RatedCharge,
}

impl DraftInvoiceLine {
    fn new(rated_charge: RatedCharge) -> Self {
        Self { rated_charge }
    }

    #[must_use]
    pub const fn rated_charge(&self) -> &RatedCharge {
        &self.rated_charge
    }

    #[must_use]
    pub const fn rated_charge_id(&self) -> &RatedChargeId {
        self.rated_charge.id()
    }

    #[must_use]
    pub const fn amount_minor(&self) -> i128 {
        self.rated_charge.amount_minor()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftInvoiceRequest {
    source_event_id: BillingEventId,
    invoice_id: BillingInvoiceId,
    customer_id: BillingCustomerId,
    organization_scope: LedgerScopeId,
    period_start_unix_ms: i64,
    period_end_unix_ms: i64,
    observed_at_unix_ms: i64,
    charges: Vec<RatedCharge>,
    currency: Currency,
    total_minor: i128,
}

impl DraftInvoiceRequest {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        source_event_id: BillingEventId,
        invoice_id: BillingInvoiceId,
        customer_id: BillingCustomerId,
        organization_scope: LedgerScopeId,
        period_start_unix_ms: i64,
        period_end_unix_ms: i64,
        observed_at_unix_ms: i64,
        mut charges: Vec<RatedCharge>,
    ) -> Result<Self, InvoicingError> {
        if period_start_unix_ms < 0
            || period_end_unix_ms < 0
            || period_start_unix_ms >= period_end_unix_ms
        {
            return Err(InvoicingError::InvalidBillingPeriod {
                period_start_unix_ms,
                period_end_unix_ms,
            });
        }
        if observed_at_unix_ms < 0 {
            return Err(InvoicingError::InvalidObservedAt(observed_at_unix_ms));
        }
        if observed_at_unix_ms < period_end_unix_ms {
            return Err(InvoicingError::ObservedBeforePeriodEnd {
                observed_at_unix_ms,
                period_end_unix_ms,
            });
        }
        if charges.is_empty() {
            return Err(InvoicingError::EmptyChargeSet);
        }

        charges.sort_by(|left, right| left.id().cmp(right.id()));
        for pair in charges.windows(2) {
            if pair[0].id() == pair[1].id() {
                return Err(InvoicingError::DuplicateRatedChargeId(pair[0].id().clone()));
            }
        }

        let currency = charges[0].currency();
        let mut total_minor = 0_i128;
        for charge in &charges {
            if charge.billing_customer_id() != &customer_id {
                return Err(InvoicingError::CustomerMismatch {
                    rated_charge_id: charge.id().clone(),
                });
            }
            if charge.currency() != currency {
                return Err(InvoicingError::CurrencyMismatch {
                    rated_charge_id: charge.id().clone(),
                    expected: currency,
                    actual: charge.currency(),
                });
            }
            if charge.window_start_unix_ms() < period_start_unix_ms
                || charge.window_end_unix_ms() > period_end_unix_ms
            {
                return Err(InvoicingError::ChargeOutsideBillingPeriod {
                    rated_charge_id: charge.id().clone(),
                    charge_start_unix_ms: charge.window_start_unix_ms(),
                    charge_end_unix_ms: charge.window_end_unix_ms(),
                    period_start_unix_ms,
                    period_end_unix_ms,
                });
            }
            if charge.rated_at_unix_ms() > observed_at_unix_ms {
                return Err(InvoicingError::ChargeRatedAfterObservation {
                    rated_charge_id: charge.id().clone(),
                    rated_at_unix_ms: charge.rated_at_unix_ms(),
                    observed_at_unix_ms,
                });
            }
            total_minor = total_minor
                .checked_add(charge.amount_minor())
                .ok_or(InvoicingError::TotalOverflow)?;
        }

        Ok(Self {
            source_event_id,
            invoice_id,
            customer_id,
            organization_scope,
            period_start_unix_ms,
            period_end_unix_ms,
            observed_at_unix_ms,
            charges,
            currency,
            total_minor,
        })
    }

    #[must_use]
    pub const fn source_event_id(&self) -> &BillingEventId {
        &self.source_event_id
    }

    #[must_use]
    pub const fn invoice_id(&self) -> &BillingInvoiceId {
        &self.invoice_id
    }

    #[must_use]
    pub const fn customer_id(&self) -> &BillingCustomerId {
        &self.customer_id
    }

    #[must_use]
    pub const fn organization_scope(&self) -> &LedgerScopeId {
        &self.organization_scope
    }

    #[must_use]
    pub const fn period_start_unix_ms(&self) -> i64 {
        self.period_start_unix_ms
    }

    #[must_use]
    pub const fn period_end_unix_ms(&self) -> i64 {
        self.period_end_unix_ms
    }

    #[must_use]
    pub const fn observed_at_unix_ms(&self) -> i64 {
        self.observed_at_unix_ms
    }

    #[must_use]
    pub fn charges(&self) -> &[RatedCharge] {
        &self.charges
    }

    #[must_use]
    pub const fn currency(&self) -> Currency {
        self.currency
    }

    #[must_use]
    pub const fn total_minor(&self) -> i128 {
        self.total_minor
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftInvoice {
    source_event_id: BillingEventId,
    invoice_id: BillingInvoiceId,
    customer_id: BillingCustomerId,
    organization_scope: LedgerScopeId,
    period_start_unix_ms: i64,
    period_end_unix_ms: i64,
    observed_at_unix_ms: i64,
    currency: Currency,
    total_minor: i128,
    lines: Vec<DraftInvoiceLine>,
}

impl DraftInvoice {
    fn from_request(request: &DraftInvoiceRequest) -> Self {
        Self {
            source_event_id: request.source_event_id.clone(),
            invoice_id: request.invoice_id.clone(),
            customer_id: request.customer_id.clone(),
            organization_scope: request.organization_scope.clone(),
            period_start_unix_ms: request.period_start_unix_ms,
            period_end_unix_ms: request.period_end_unix_ms,
            observed_at_unix_ms: request.observed_at_unix_ms,
            currency: request.currency,
            total_minor: request.total_minor,
            lines: request
                .charges
                .iter()
                .cloned()
                .map(DraftInvoiceLine::new)
                .collect(),
        }
    }

    #[must_use]
    pub const fn source_event_id(&self) -> &BillingEventId {
        &self.source_event_id
    }

    #[must_use]
    pub const fn invoice_id(&self) -> &BillingInvoiceId {
        &self.invoice_id
    }

    #[must_use]
    pub const fn customer_id(&self) -> &BillingCustomerId {
        &self.customer_id
    }

    #[must_use]
    pub const fn organization_scope(&self) -> &LedgerScopeId {
        &self.organization_scope
    }

    #[must_use]
    pub const fn period_start_unix_ms(&self) -> i64 {
        self.period_start_unix_ms
    }

    #[must_use]
    pub const fn period_end_unix_ms(&self) -> i64 {
        self.period_end_unix_ms
    }

    #[must_use]
    pub const fn observed_at_unix_ms(&self) -> i64 {
        self.observed_at_unix_ms
    }

    #[must_use]
    pub const fn currency(&self) -> Currency {
        self.currency
    }

    #[must_use]
    pub const fn total_minor(&self) -> i128 {
        self.total_minor
    }

    #[must_use]
    pub fn lines(&self) -> &[DraftInvoiceLine] {
        &self.lines
    }

    #[must_use]
    pub fn to_billing_event(&self) -> InvoiceEvent {
        InvoiceEvent::new(
            self.source_event_id.clone(),
            self.organization_scope.clone(),
            self.invoice_id.clone(),
            self.customer_id.clone(),
            InvoiceStatus::Draft,
            self.currency,
            self.total_minor,
            self.total_minor,
            0,
            None,
            self.observed_at_unix_ms,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DraftAssemblyOutcome {
    Created { draft: DraftInvoice },
    Replayed { draft: DraftInvoice },
}

impl DraftAssemblyOutcome {
    #[must_use]
    pub const fn draft(&self) -> &DraftInvoice {
        match self {
            Self::Created { draft } | Self::Replayed { draft } => draft,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StoredDraft {
    request: DraftInvoiceRequest,
    draft: DraftInvoice,
}

#[derive(Debug, Clone, Default)]
pub struct DraftInvoiceRegistry {
    events: BTreeMap<BillingEventId, StoredDraft>,
    invoices: BTreeMap<BillingInvoiceId, BillingEventId>,
    charge_bindings: BTreeMap<RatedChargeId, BillingInvoiceId>,
}

impl DraftInvoiceRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn event_count(&self) -> usize {
        self.events.len()
    }

    #[must_use]
    pub fn invoice_count(&self) -> usize {
        self.invoices.len()
    }

    #[must_use]
    pub fn charge_binding_count(&self) -> usize {
        self.charge_bindings.len()
    }

    #[must_use]
    pub fn draft_for_event(&self, event_id: &BillingEventId) -> Option<&DraftInvoice> {
        self.events.get(event_id).map(|stored| &stored.draft)
    }

    pub fn assemble(
        &mut self,
        request: DraftInvoiceRequest,
    ) -> Result<DraftAssemblyOutcome, InvoicingError> {
        if let Some(existing) = self.events.get(request.source_event_id()) {
            return if existing.request == request {
                Ok(DraftAssemblyOutcome::Replayed {
                    draft: existing.draft.clone(),
                })
            } else {
                Err(InvoicingError::SourceEventConflict(
                    request.source_event_id().clone(),
                ))
            };
        }

        if let Some(existing_event_id) = self.invoices.get(request.invoice_id()) {
            return Err(InvoicingError::InvoiceIdConflict {
                invoice_id: request.invoice_id().clone(),
                existing_event_id: existing_event_id.clone(),
            });
        }

        let mut unique_charge_ids = BTreeSet::new();
        for charge in request.charges() {
            if !unique_charge_ids.insert(charge.id().clone()) {
                return Err(InvoicingError::DuplicateRatedChargeId(charge.id().clone()));
            }
            if let Some(existing_invoice_id) = self.charge_bindings.get(charge.id()) {
                return Err(InvoicingError::RatedChargeAlreadyBound {
                    rated_charge_id: charge.id().clone(),
                    existing_invoice_id: existing_invoice_id.clone(),
                });
            }
        }

        let draft = DraftInvoice::from_request(&request);
        let event_id = request.source_event_id().clone();
        let invoice_id = request.invoice_id().clone();
        for charge in request.charges() {
            self.charge_bindings
                .insert(charge.id().clone(), invoice_id.clone());
        }
        self.invoices.insert(invoice_id, event_id.clone());
        self.events.insert(
            event_id,
            StoredDraft {
                request,
                draft: draft.clone(),
            },
        );
        Ok(DraftAssemblyOutcome::Created { draft })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvoicingError {
    InvalidBillingPeriod {
        period_start_unix_ms: i64,
        period_end_unix_ms: i64,
    },
    InvalidObservedAt(i64),
    ObservedBeforePeriodEnd {
        observed_at_unix_ms: i64,
        period_end_unix_ms: i64,
    },
    EmptyChargeSet,
    DuplicateRatedChargeId(RatedChargeId),
    CustomerMismatch {
        rated_charge_id: RatedChargeId,
    },
    CurrencyMismatch {
        rated_charge_id: RatedChargeId,
        expected: Currency,
        actual: Currency,
    },
    ChargeOutsideBillingPeriod {
        rated_charge_id: RatedChargeId,
        charge_start_unix_ms: i64,
        charge_end_unix_ms: i64,
        period_start_unix_ms: i64,
        period_end_unix_ms: i64,
    },
    ChargeRatedAfterObservation {
        rated_charge_id: RatedChargeId,
        rated_at_unix_ms: i64,
        observed_at_unix_ms: i64,
    },
    TotalOverflow,
    SourceEventConflict(BillingEventId),
    InvoiceIdConflict {
        invoice_id: BillingInvoiceId,
        existing_event_id: BillingEventId,
    },
    RatedChargeAlreadyBound {
        rated_charge_id: RatedChargeId,
        existing_invoice_id: BillingInvoiceId,
    },
}

impl Display for InvoicingError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidBillingPeriod {
                period_start_unix_ms,
                period_end_unix_ms,
            } => write!(
                formatter,
                "invalid billing period [{period_start_unix_ms}, {period_end_unix_ms})"
            ),
            Self::InvalidObservedAt(value) => {
                write!(
                    formatter,
                    "observed_at_unix_ms must not be negative: {value}"
                )
            }
            Self::ObservedBeforePeriodEnd {
                observed_at_unix_ms,
                period_end_unix_ms,
            } => write!(
                formatter,
                "draft observation {observed_at_unix_ms} precedes billing-period end {period_end_unix_ms}"
            ),
            Self::EmptyChargeSet => write!(
                formatter,
                "draft invoice requires at least one rated charge"
            ),
            Self::DuplicateRatedChargeId(id) => {
                write!(formatter, "duplicate rated charge id: {}", id.as_str())
            }
            Self::CustomerMismatch { rated_charge_id } => write!(
                formatter,
                "rated charge {} belongs to a different billing customer",
                rated_charge_id.as_str()
            ),
            Self::CurrencyMismatch {
                rated_charge_id,
                expected,
                actual,
            } => write!(
                formatter,
                "rated charge {} has currency {actual}, expected {expected}",
                rated_charge_id.as_str()
            ),
            Self::ChargeOutsideBillingPeriod {
                rated_charge_id,
                charge_start_unix_ms,
                charge_end_unix_ms,
                period_start_unix_ms,
                period_end_unix_ms,
            } => write!(
                formatter,
                "rated charge {} window [{charge_start_unix_ms}, {charge_end_unix_ms}) is outside billing period [{period_start_unix_ms}, {period_end_unix_ms})",
                rated_charge_id.as_str()
            ),
            Self::ChargeRatedAfterObservation {
                rated_charge_id,
                rated_at_unix_ms,
                observed_at_unix_ms,
            } => write!(
                formatter,
                "rated charge {} was rated at {rated_at_unix_ms}, after draft observation {observed_at_unix_ms}",
                rated_charge_id.as_str()
            ),
            Self::TotalOverflow => write!(formatter, "draft invoice total overflow"),
            Self::SourceEventConflict(id) => {
                write!(formatter, "billing source event conflict: {}", id.as_str())
            }
            Self::InvoiceIdConflict {
                invoice_id,
                existing_event_id,
            } => write!(
                formatter,
                "invoice {} already has draft history under event {}",
                invoice_id.as_str(),
                existing_event_id.as_str()
            ),
            Self::RatedChargeAlreadyBound {
                rated_charge_id,
                existing_invoice_id,
            } => write!(
                formatter,
                "rated charge {} is already bound to invoice {}",
                rated_charge_id.as_str(),
                existing_invoice_id.as_str()
            ),
        }
    }
}

impl Error for InvoicingError {}
