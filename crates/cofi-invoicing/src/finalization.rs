use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use cofi_billing::{BillingEventId, BillingInvoiceId, InvoiceEvent, InvoiceStatus};

use crate::DraftInvoice;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinalizationRequest {
    source_event_id: BillingEventId,
    draft: DraftInvoice,
    finalized_at_unix_ms: i64,
    observed_at_unix_ms: i64,
}

impl FinalizationRequest {
    pub fn new(
        source_event_id: BillingEventId,
        draft: DraftInvoice,
        finalized_at_unix_ms: i64,
        observed_at_unix_ms: i64,
    ) -> Result<Self, FinalizationError> {
        if source_event_id == *draft.source_event_id() {
            return Err(FinalizationError::ReusedDraftSourceEvent(source_event_id));
        }
        if draft.total_minor() <= 0 {
            return Err(FinalizationError::NonPositiveDraftTotal {
                invoice_id: draft.invoice_id().clone(),
                total_minor: draft.total_minor(),
            });
        }
        if finalized_at_unix_ms < 0 {
            return Err(FinalizationError::InvalidFinalizedAt(finalized_at_unix_ms));
        }
        let draft_ready_at_unix_ms = draft.period_end_unix_ms().max(draft.observed_at_unix_ms());
        if finalized_at_unix_ms < draft_ready_at_unix_ms {
            return Err(FinalizationError::FinalizedBeforeDraftReady {
                finalized_at_unix_ms,
                draft_ready_at_unix_ms,
            });
        }
        if observed_at_unix_ms < 0 {
            return Err(FinalizationError::InvalidObservedAt(observed_at_unix_ms));
        }
        if observed_at_unix_ms < finalized_at_unix_ms {
            return Err(FinalizationError::ObservedBeforeFinalized {
                observed_at_unix_ms,
                finalized_at_unix_ms,
            });
        }

        Ok(Self {
            source_event_id,
            draft,
            finalized_at_unix_ms,
            observed_at_unix_ms,
        })
    }

    #[must_use]
    pub const fn source_event_id(&self) -> &BillingEventId {
        &self.source_event_id
    }

    #[must_use]
    pub const fn draft(&self) -> &DraftInvoice {
        &self.draft
    }

    #[must_use]
    pub const fn finalized_at_unix_ms(&self) -> i64 {
        self.finalized_at_unix_ms
    }

    #[must_use]
    pub const fn observed_at_unix_ms(&self) -> i64 {
        self.observed_at_unix_ms
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinalizedInvoice {
    source_event_id: BillingEventId,
    draft: DraftInvoice,
    finalized_at_unix_ms: i64,
    observed_at_unix_ms: i64,
}

impl FinalizedInvoice {
    fn from_request(request: &FinalizationRequest) -> Self {
        Self {
            source_event_id: request.source_event_id.clone(),
            draft: request.draft.clone(),
            finalized_at_unix_ms: request.finalized_at_unix_ms,
            observed_at_unix_ms: request.observed_at_unix_ms,
        }
    }

    #[must_use]
    pub const fn source_event_id(&self) -> &BillingEventId {
        &self.source_event_id
    }

    #[must_use]
    pub const fn draft(&self) -> &DraftInvoice {
        &self.draft
    }

    #[must_use]
    pub const fn finalized_at_unix_ms(&self) -> i64 {
        self.finalized_at_unix_ms
    }

    #[must_use]
    pub const fn observed_at_unix_ms(&self) -> i64 {
        self.observed_at_unix_ms
    }

    #[must_use]
    pub fn to_billing_event(&self) -> InvoiceEvent {
        InvoiceEvent::new(
            self.source_event_id.clone(),
            self.draft.organization_scope().clone(),
            self.draft.invoice_id().clone(),
            self.draft.customer_id().clone(),
            InvoiceStatus::Finalized,
            self.draft.currency(),
            self.draft.total_minor(),
            self.draft.total_minor(),
            0,
            Some(self.finalized_at_unix_ms),
            self.observed_at_unix_ms,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FinalizationOutcome {
    Created { finalized: FinalizedInvoice },
    Replayed { finalized: FinalizedInvoice },
}

impl FinalizationOutcome {
    #[must_use]
    pub const fn finalized(&self) -> &FinalizedInvoice {
        match self {
            Self::Created { finalized } | Self::Replayed { finalized } => finalized,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StoredFinalization {
    request: FinalizationRequest,
    finalized: FinalizedInvoice,
}

#[derive(Debug, Clone, Default)]
pub struct FinalizationRegistry {
    events: BTreeMap<BillingEventId, StoredFinalization>,
    invoices: BTreeMap<BillingInvoiceId, BillingEventId>,
}

impl FinalizationRegistry {
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
    pub fn finalized_for_event(&self, event_id: &BillingEventId) -> Option<&FinalizedInvoice> {
        self.events.get(event_id).map(|stored| &stored.finalized)
    }

    pub fn finalize(
        &mut self,
        request: FinalizationRequest,
    ) -> Result<FinalizationOutcome, FinalizationError> {
        if let Some(existing) = self.events.get(request.source_event_id()) {
            return if existing.request == request {
                Ok(FinalizationOutcome::Replayed {
                    finalized: existing.finalized.clone(),
                })
            } else {
                Err(FinalizationError::SourceEventConflict(
                    request.source_event_id().clone(),
                ))
            };
        }

        if let Some(existing_event_id) = self.invoices.get(request.draft().invoice_id()) {
            return Err(FinalizationError::InvoiceAlreadyFinalized {
                invoice_id: request.draft().invoice_id().clone(),
                existing_event_id: existing_event_id.clone(),
            });
        }

        let finalized = FinalizedInvoice::from_request(&request);
        let event_id = request.source_event_id().clone();
        let invoice_id = request.draft().invoice_id().clone();
        self.invoices.insert(invoice_id, event_id.clone());
        self.events.insert(
            event_id,
            StoredFinalization {
                request,
                finalized: finalized.clone(),
            },
        );
        Ok(FinalizationOutcome::Created { finalized })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FinalizationError {
    ReusedDraftSourceEvent(BillingEventId),
    NonPositiveDraftTotal {
        invoice_id: BillingInvoiceId,
        total_minor: i128,
    },
    InvalidFinalizedAt(i64),
    FinalizedBeforeDraftReady {
        finalized_at_unix_ms: i64,
        draft_ready_at_unix_ms: i64,
    },
    InvalidObservedAt(i64),
    ObservedBeforeFinalized {
        observed_at_unix_ms: i64,
        finalized_at_unix_ms: i64,
    },
    SourceEventConflict(BillingEventId),
    InvoiceAlreadyFinalized {
        invoice_id: BillingInvoiceId,
        existing_event_id: BillingEventId,
    },
}

impl Display for FinalizationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReusedDraftSourceEvent(event_id) => write!(
                formatter,
                "finalization event {} reuses the draft assembly event identity",
                event_id.as_str()
            ),
            Self::NonPositiveDraftTotal {
                invoice_id,
                total_minor,
            } => write!(
                formatter,
                "invoice {} cannot be finalized with non-positive total {total_minor}",
                invoice_id.as_str()
            ),
            Self::InvalidFinalizedAt(value) => write!(
                formatter,
                "finalized_at_unix_ms must not be negative: {value}"
            ),
            Self::FinalizedBeforeDraftReady {
                finalized_at_unix_ms,
                draft_ready_at_unix_ms,
            } => write!(
                formatter,
                "finalization {finalized_at_unix_ms} precedes draft readiness {draft_ready_at_unix_ms}"
            ),
            Self::InvalidObservedAt(value) => write!(
                formatter,
                "finalization observed_at_unix_ms must not be negative: {value}"
            ),
            Self::ObservedBeforeFinalized {
                observed_at_unix_ms,
                finalized_at_unix_ms,
            } => write!(
                formatter,
                "finalization observation {observed_at_unix_ms} precedes finalized time {finalized_at_unix_ms}"
            ),
            Self::SourceEventConflict(event_id) => write!(
                formatter,
                "finalization source event conflict: {}",
                event_id.as_str()
            ),
            Self::InvoiceAlreadyFinalized {
                invoice_id,
                existing_event_id,
            } => write!(
                formatter,
                "invoice {} already has finalization history under event {}",
                invoice_id.as_str(),
                existing_event_id.as_str()
            ),
        }
    }
}

impl Error for FinalizationError {}
