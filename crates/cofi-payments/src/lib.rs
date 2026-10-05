use std::error::Error;
use std::fmt::{Display, Formatter};

mod payout;
pub use payout::*;

use cofi_billing::{BillingError, BillingInvoiceId, journal_entry_id_for_invoice};
use cofi_ledger::{
    AccountId, AccountKind, CommitOutcome, Currency, EntryMetadata, JournalEntry, JournalEntryId,
    Ledger, LedgerError, LedgerScopeId, LedgerStateError, Posting, Side,
};

macro_rules! payment_id {
    ($name:ident, $label:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, PaymentError> {
                let value = value.into();
                if value.trim().is_empty() {
                    return Err(PaymentError::EmptyIdentifier($label));
                }
                Ok(Self(value))
            }

            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}

payment_id!(PaymentEventId, "payment_event_id");
payment_id!(PaymentId, "payment_id");
payment_id!(ConnectorTransactionId, "connector_transaction_id");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaymentStatus {
    Authorized,
    Pending,
    Charged,
    PartialCharged,
    Failed,
    Voided,
    AutoRefunded,
    VoidedPostCharge,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentEvent {
    source_event_id: PaymentEventId,
    organization_scope: LedgerScopeId,
    invoice_id: BillingInvoiceId,
    payment_id: PaymentId,
    connector_transaction_id: ConnectorTransactionId,
    status: PaymentStatus,
    currency: Currency,
    amount_captured_minor: i128,
    captured_at_unix_ms: Option<i64>,
    observed_at_unix_ms: i64,
}

impl PaymentEvent {
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        source_event_id: PaymentEventId,
        organization_scope: LedgerScopeId,
        invoice_id: BillingInvoiceId,
        payment_id: PaymentId,
        connector_transaction_id: ConnectorTransactionId,
        status: PaymentStatus,
        currency: Currency,
        amount_captured_minor: i128,
        captured_at_unix_ms: Option<i64>,
        observed_at_unix_ms: i64,
    ) -> Self {
        Self {
            source_event_id,
            organization_scope,
            invoice_id,
            payment_id,
            connector_transaction_id,
            status,
            currency,
            amount_captured_minor,
            captured_at_unix_ms,
            observed_at_unix_ms,
        }
    }

    #[must_use]
    pub fn invoice_id(&self) -> &BillingInvoiceId {
        &self.invoice_id
    }

    #[must_use]
    pub fn payment_id(&self) -> &PaymentId {
        &self.payment_id
    }

    #[must_use]
    pub fn connector_transaction_id(&self) -> &ConnectorTransactionId {
        &self.connector_transaction_id
    }

    #[must_use]
    pub fn organization_scope(&self) -> &LedgerScopeId {
        &self.organization_scope
    }

    #[must_use]
    pub const fn status(&self) -> PaymentStatus {
        self.status
    }

    #[must_use]
    pub const fn currency(&self) -> Currency {
        self.currency
    }

    #[must_use]
    pub const fn amount_captured_minor(&self) -> i128 {
        self.amount_captured_minor
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentLedgerAccounts {
    receivable: AccountId,
    processor_clearing: AccountId,
}

impl PaymentLedgerAccounts {
    pub fn new(receivable: AccountId, processor_clearing: AccountId) -> Result<Self, PaymentError> {
        if receivable == processor_clearing {
            return Err(PaymentError::SamePostingAccount(receivable));
        }
        Ok(Self {
            receivable,
            processor_clearing,
        })
    }

    #[must_use]
    pub fn receivable(&self) -> &AccountId {
        &self.receivable
    }

    #[must_use]
    pub fn processor_clearing(&self) -> &AccountId {
        &self.processor_clearing
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaymentApplyOutcome {
    IgnoredNoCapture { status: PaymentStatus },
    Committed { journal_entry_id: JournalEntryId },
    Replayed { journal_entry_id: JournalEntryId },
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PaymentLedgerBridge;

impl PaymentLedgerBridge {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    pub fn apply(
        &self,
        event: &PaymentEvent,
        accounts: &PaymentLedgerAccounts,
        ledger: &mut Ledger,
    ) -> Result<PaymentApplyOutcome, PaymentError> {
        match event.status {
            PaymentStatus::Authorized
            | PaymentStatus::Pending
            | PaymentStatus::Failed
            | PaymentStatus::Voided => return validate_no_capture(event),
            PaymentStatus::PartialCharged
            | PaymentStatus::AutoRefunded
            | PaymentStatus::VoidedPostCharge => {
                return Err(PaymentError::UnsupportedPaymentStatus(event.status));
            }
            PaymentStatus::Charged => {}
        }

        if event.amount_captured_minor <= 0 {
            return Err(PaymentError::InvalidCapturedAmount(
                event.amount_captured_minor,
            ));
        }
        let captured_at = event
            .captured_at_unix_ms
            .ok_or(PaymentError::MissingCapturedAt)?;

        validate_account(
            ledger,
            accounts.receivable(),
            event.organization_scope(),
            event.currency,
        )?;
        validate_account(
            ledger,
            accounts.processor_clearing(),
            event.organization_scope(),
            event.currency,
        )?;

        let invoice_entry_id = journal_entry_id_for_invoice(event.invoice_id())
            .map_err(PaymentError::BillingReference)?;
        let invoice_entry = ledger
            .entry(&invoice_entry_id)
            .ok_or_else(|| PaymentError::MissingInvoiceJournalEntry(invoice_entry_id.clone()))?;
        let invoice_receivable = invoice_receivable_amount(
            invoice_entry,
            &invoice_entry_id,
            accounts.receivable(),
            event.currency,
        )?;

        let captured_amount = event.amount_captured_minor as u128;
        if invoice_receivable != captured_amount {
            return Err(PaymentError::CapturedAmountMismatch {
                expected: invoice_receivable,
                actual: captured_amount,
            });
        }
        if captured_at < invoice_entry.effective_at_unix_ms() {
            return Err(PaymentError::CapturePrecedesInvoiceFinalization {
                captured_at,
                invoice_finalized_at: invoice_entry.effective_at_unix_ms(),
            });
        }

        let entry_id = journal_entry_id_for_payment(event.payment_id())?;
        let metadata = EntryMetadata::new(
            Some(payment_payload_correlation(event)),
            Some(event.source_event_id.as_str().to_owned()),
        )
        .and_then(|metadata| {
            metadata.with_business_key(Some(invoice_application_business_key(event.invoice_id())))
        })
        .map_err(PaymentError::LedgerBuild)?;
        let postings = vec![
            Posting::new(
                accounts.processor_clearing().clone(),
                event.currency,
                Side::Debit,
                event.amount_captured_minor,
            )
            .map_err(PaymentError::LedgerBuild)?,
            Posting::new(
                accounts.receivable().clone(),
                event.currency,
                Side::Credit,
                event.amount_captured_minor,
            )
            .map_err(PaymentError::LedgerBuild)?,
        ];

        let entry = JournalEntry::new(
            entry_id.clone(),
            postings,
            captured_at,
            event.observed_at_unix_ms,
            metadata,
        )
        .map_err(PaymentError::LedgerBuild)?;

        match ledger.commit(entry).map_err(PaymentError::LedgerCommit)? {
            CommitOutcome::Committed => Ok(PaymentApplyOutcome::Committed {
                journal_entry_id: entry_id,
            }),
            CommitOutcome::Replayed => Ok(PaymentApplyOutcome::Replayed {
                journal_entry_id: entry_id,
            }),
        }
    }
}

fn validate_no_capture(event: &PaymentEvent) -> Result<PaymentApplyOutcome, PaymentError> {
    if event.amount_captured_minor != 0 || event.captured_at_unix_ms.is_some() {
        return Err(PaymentError::InconsistentCaptureState {
            status: event.status,
            amount_captured: event.amount_captured_minor,
            has_captured_at: event.captured_at_unix_ms.is_some(),
        });
    }
    Ok(PaymentApplyOutcome::IgnoredNoCapture {
        status: event.status,
    })
}

pub fn journal_entry_id_for_payment(
    payment_id: &PaymentId,
) -> Result<JournalEntryId, PaymentError> {
    JournalEntryId::new(format!("payments:payment:{}:charged", payment_id.as_str()))
        .map_err(PaymentError::LedgerBuild)
}

fn invoice_application_business_key(invoice_id: &BillingInvoiceId) -> String {
    format!("payments:invoice:{}:full-capture", invoice_id.as_str())
}

fn payment_payload_correlation(event: &PaymentEvent) -> String {
    format!(
        "payment:{}:{}:{}:{}:{}:{}:{}",
        event.invoice_id.as_str(),
        event.payment_id.as_str(),
        event.connector_transaction_id.as_str(),
        event.organization_scope.as_str(),
        event.currency.code(),
        event.amount_captured_minor,
        event.captured_at_unix_ms.unwrap_or_default(),
    )
}

fn validate_account(
    ledger: &Ledger,
    account_id: &AccountId,
    expected_scope: &LedgerScopeId,
    expected_currency: Currency,
) -> Result<(), PaymentError> {
    let account = ledger
        .account(account_id)
        .ok_or_else(|| PaymentError::UnknownLedgerAccount(account_id.clone()))?;
    if account.kind() != AccountKind::Asset {
        return Err(PaymentError::AccountKindMismatch {
            account_id: account_id.clone(),
            expected: AccountKind::Asset,
            actual: account.kind(),
        });
    }
    if account.scope_id() != expected_scope {
        return Err(PaymentError::AccountScopeMismatch {
            account_id: account_id.clone(),
            expected: expected_scope.clone(),
            actual: account.scope_id().clone(),
        });
    }
    if account.currency() != expected_currency {
        return Err(PaymentError::AccountCurrencyMismatch {
            account_id: account_id.clone(),
            expected: expected_currency,
            actual: account.currency(),
        });
    }
    Ok(())
}

fn invoice_receivable_amount(
    entry: &JournalEntry,
    invoice_entry_id: &JournalEntryId,
    receivable: &AccountId,
    expected_currency: Currency,
) -> Result<u128, PaymentError> {
    let mut matches = entry
        .postings()
        .iter()
        .filter(|posting| posting.account_id() == receivable);
    let posting = matches
        .next()
        .ok_or_else(|| PaymentError::InvoiceReceivablePostingMissing {
            invoice_entry_id: invoice_entry_id.clone(),
            account_id: receivable.clone(),
        })?;
    if matches.next().is_some()
        || posting.side() != Side::Debit
        || posting.currency() != expected_currency
    {
        return Err(PaymentError::InvoiceReceivablePostingInvalid {
            invoice_entry_id: invoice_entry_id.clone(),
            account_id: receivable.clone(),
        });
    }
    Ok(posting.amount().value())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaymentError {
    EmptyIdentifier(&'static str),
    SamePostingAccount(AccountId),
    UnsupportedPaymentStatus(PaymentStatus),
    InconsistentCaptureState {
        status: PaymentStatus,
        amount_captured: i128,
        has_captured_at: bool,
    },
    MissingCapturedAt,
    InvalidCapturedAmount(i128),
    BillingReference(BillingError),
    MissingInvoiceJournalEntry(JournalEntryId),
    InvoiceReceivablePostingMissing {
        invoice_entry_id: JournalEntryId,
        account_id: AccountId,
    },
    InvoiceReceivablePostingInvalid {
        invoice_entry_id: JournalEntryId,
        account_id: AccountId,
    },
    CapturedAmountMismatch {
        expected: u128,
        actual: u128,
    },
    CapturePrecedesInvoiceFinalization {
        captured_at: i64,
        invoice_finalized_at: i64,
    },
    UnknownLedgerAccount(AccountId),
    AccountKindMismatch {
        account_id: AccountId,
        expected: AccountKind,
        actual: AccountKind,
    },
    AccountScopeMismatch {
        account_id: AccountId,
        expected: LedgerScopeId,
        actual: LedgerScopeId,
    },
    AccountCurrencyMismatch {
        account_id: AccountId,
        expected: Currency,
        actual: Currency,
    },
    LedgerBuild(LedgerError),
    LedgerCommit(LedgerStateError),
}

impl Display for PaymentError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyIdentifier(name) => write!(f, "{name} must not be empty"),
            Self::SamePostingAccount(id) => write!(
                f,
                "receivable and processor clearing accounts must be distinct: {}",
                id.as_str()
            ),
            Self::UnsupportedPaymentStatus(status) => {
                write!(f, "payment status is unsupported in P05: {status:?}")
            }
            Self::InconsistentCaptureState {
                status,
                amount_captured,
                has_captured_at,
            } => write!(
                f,
                "non-charged status has inconsistent capture data: status={status:?}, amount_captured={amount_captured}, has_captured_at={has_captured_at}"
            ),
            Self::MissingCapturedAt => f.write_str("charged payment requires captured_at"),
            Self::InvalidCapturedAmount(amount) => {
                write!(
                    f,
                    "charged payment amount_captured must be positive: {amount}"
                )
            }
            Self::BillingReference(error) => write!(f, "invalid billing reference: {error}"),
            Self::MissingInvoiceJournalEntry(id) => write!(
                f,
                "referenced invoice has no canonical billing journal entry: {}",
                id.as_str()
            ),
            Self::InvoiceReceivablePostingMissing {
                invoice_entry_id,
                account_id,
            } => write!(
                f,
                "invoice entry {} does not debit configured receivable account {}",
                invoice_entry_id.as_str(),
                account_id.as_str()
            ),
            Self::InvoiceReceivablePostingInvalid {
                invoice_entry_id,
                account_id,
            } => write!(
                f,
                "invoice entry {} has an invalid receivable posting for account {}",
                invoice_entry_id.as_str(),
                account_id.as_str()
            ),
            Self::CapturedAmountMismatch { expected, actual } => write!(
                f,
                "P05 supports full capture only: expected invoice receivable {expected}, captured {actual}"
            ),
            Self::CapturePrecedesInvoiceFinalization {
                captured_at,
                invoice_finalized_at,
            } => write!(
                f,
                "payment capture precedes canonical invoice finalization: captured_at={captured_at}, invoice_finalized_at={invoice_finalized_at}"
            ),
            Self::UnknownLedgerAccount(id) => write!(
                f,
                "payment ledger account is not registered: {}",
                id.as_str()
            ),
            Self::AccountKindMismatch {
                account_id,
                expected,
                actual,
            } => write!(
                f,
                "payment ledger account {} has kind {actual:?}; expected {expected:?}",
                account_id.as_str()
            ),
            Self::AccountScopeMismatch {
                account_id,
                expected,
                actual,
            } => write!(
                f,
                "payment ledger account {} has scope {}; expected {}",
                account_id.as_str(),
                actual.as_str(),
                expected.as_str()
            ),
            Self::AccountCurrencyMismatch {
                account_id,
                expected,
                actual,
            } => write!(
                f,
                "payment ledger account {} has currency {actual}; expected {expected}",
                account_id.as_str()
            ),
            Self::LedgerBuild(error) => write!(f, "failed to build payment journal entry: {error}"),
            Self::LedgerCommit(error) => {
                write!(f, "failed to commit payment journal entry: {error}")
            }
        }
    }
}

impl Error for PaymentError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::BillingReference(error) => Some(error),
            Self::LedgerBuild(error) => Some(error),
            Self::LedgerCommit(error) => Some(error),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use cofi_billing::{
        BillingCustomerId, BillingEventId, BillingLedgerAccounts, BillingLedgerBridge,
        InvoiceEvent, InvoiceStatus,
    };
    use cofi_ledger::{Account, AccountBalance};

    fn usd() -> Currency {
        Currency::new("USD").unwrap()
    }

    fn eur() -> Currency {
        Currency::new("EUR").unwrap()
    }

    fn scope(value: &str) -> LedgerScopeId {
        LedgerScopeId::new(value).unwrap()
    }

    fn account_id(value: &str) -> AccountId {
        AccountId::new(value).unwrap()
    }

    fn invoice_id(value: &str) -> BillingInvoiceId {
        BillingInvoiceId::new(value).unwrap()
    }

    fn accounts() -> PaymentLedgerAccounts {
        PaymentLedgerAccounts::new(
            account_id("accounts-receivable"),
            account_id("processor-clearing"),
        )
        .unwrap()
    }

    fn register_account(
        ledger: &mut Ledger,
        id: &str,
        scope_id: &str,
        kind: AccountKind,
        currency: Currency,
    ) {
        ledger
            .register_account(Account::new(
                account_id(id),
                scope(scope_id),
                kind,
                currency,
            ))
            .unwrap();
    }

    fn ledger_with_invoice(amount: i128, invoice: &str) -> Ledger {
        let mut ledger = Ledger::new();
        register_account(
            &mut ledger,
            "accounts-receivable",
            "org-1",
            AccountKind::Asset,
            usd(),
        );
        register_account(&mut ledger, "revenue", "org-1", AccountKind::Revenue, usd());
        register_account(
            &mut ledger,
            "processor-clearing",
            "org-1",
            AccountKind::Asset,
            usd(),
        );
        let billing_event = InvoiceEvent::new(
            BillingEventId::new(format!("billing-event-{invoice}")).unwrap(),
            scope("org-1"),
            invoice_id(invoice),
            BillingCustomerId::new("customer-1").unwrap(),
            InvoiceStatus::Finalized,
            usd(),
            amount,
            amount,
            0,
            Some(1_700_000_000_000),
            1_700_000_000_100,
        );
        let billing_accounts =
            BillingLedgerAccounts::new(account_id("accounts-receivable"), account_id("revenue"))
                .unwrap();
        BillingLedgerBridge::new()
            .apply(&billing_event, &billing_accounts, &mut ledger)
            .unwrap();
        ledger
    }

    fn payment_event(
        status: PaymentStatus,
        source_event: &str,
        payment: &str,
        invoice: &str,
        amount: i128,
        captured_at: Option<i64>,
    ) -> PaymentEvent {
        PaymentEvent::new(
            PaymentEventId::new(source_event).unwrap(),
            scope("org-1"),
            invoice_id(invoice),
            PaymentId::new(payment).unwrap(),
            ConnectorTransactionId::new(format!("connector-{payment}")).unwrap(),
            status,
            usd(),
            amount,
            captured_at,
            1_700_000_001_000,
        )
    }

    fn charged_event(
        source_event: &str,
        payment: &str,
        invoice: &str,
        amount: i128,
    ) -> PaymentEvent {
        payment_event(
            PaymentStatus::Charged,
            source_event,
            payment,
            invoice,
            amount,
            Some(1_700_000_000_500),
        )
    }

    fn balances(ledger: &Ledger) -> (AccountBalance, AccountBalance) {
        (
            ledger.balance(&account_id("accounts-receivable")).unwrap(),
            ledger.balance(&account_id("processor-clearing")).unwrap(),
        )
    }

    #[test]
    fn full_charged_payment_moves_receivable_to_processor_clearing() {
        let mut ledger = ledger_with_invoice(10_000, "invoice-1");
        let outcome = PaymentLedgerBridge::new()
            .apply(
                &charged_event("payment-event-1", "payment-1", "invoice-1", 10_000),
                &accounts(),
                &mut ledger,
            )
            .unwrap();

        assert_eq!(
            outcome,
            PaymentApplyOutcome::Committed {
                journal_entry_id: journal_entry_id_for_payment(
                    &PaymentId::new("payment-1").unwrap()
                )
                .unwrap(),
            }
        );
        let (receivable, clearing) = balances(&ledger);
        assert_eq!(receivable.debits(), 10_000);
        assert_eq!(receivable.credits(), 10_000);
        assert_eq!(clearing.debits(), 10_000);
        assert_eq!(clearing.credits(), 0);
        assert_eq!(ledger.entry_count(), 2);
    }

    #[test]
    fn payment_cannot_post_before_canonical_invoice_exists() {
        let mut ledger = Ledger::new();
        register_account(
            &mut ledger,
            "accounts-receivable",
            "org-1",
            AccountKind::Asset,
            usd(),
        );
        register_account(
            &mut ledger,
            "processor-clearing",
            "org-1",
            AccountKind::Asset,
            usd(),
        );

        let expected_id = journal_entry_id_for_invoice(&invoice_id("invoice-missing")).unwrap();
        assert_eq!(
            PaymentLedgerBridge::new().apply(
                &charged_event(
                    "payment-event-missing",
                    "payment-missing",
                    "invoice-missing",
                    10_000
                ),
                &accounts(),
                &mut ledger,
            ),
            Err(PaymentError::MissingInvoiceJournalEntry(expected_id))
        );
        assert_eq!(ledger.entry_count(), 0);
        let (receivable, clearing) = balances(&ledger);
        assert_eq!(receivable, AccountBalance::default());
        assert_eq!(clearing, AccountBalance::default());
    }

    #[test]
    fn full_capture_amount_must_match_invoice_receivable() {
        let mut ledger = ledger_with_invoice(10_000, "invoice-1");
        let before = balances(&ledger);
        assert_eq!(
            PaymentLedgerBridge::new().apply(
                &charged_event("payment-event-short", "payment-short", "invoice-1", 9_000),
                &accounts(),
                &mut ledger,
            ),
            Err(PaymentError::CapturedAmountMismatch {
                expected: 10_000,
                actual: 9_000,
            })
        );
        assert_eq!(ledger.entry_count(), 1);
        assert_eq!(balances(&ledger), before);
    }

    #[test]
    fn no_capture_states_have_zero_economic_effect() {
        for status in [
            PaymentStatus::Authorized,
            PaymentStatus::Pending,
            PaymentStatus::Failed,
            PaymentStatus::Voided,
        ] {
            let mut ledger = ledger_with_invoice(10_000, "invoice-1");
            let before = balances(&ledger);
            let outcome = PaymentLedgerBridge::new()
                .apply(
                    &payment_event(
                        status,
                        "event-no-capture",
                        "payment-1",
                        "invoice-1",
                        0,
                        None,
                    ),
                    &accounts(),
                    &mut ledger,
                )
                .unwrap();
            assert_eq!(outcome, PaymentApplyOutcome::IgnoredNoCapture { status });
            assert_eq!(ledger.entry_count(), 1);
            assert_eq!(balances(&ledger), before);
        }
    }

    #[test]
    fn non_charged_status_with_capture_data_fails_closed() {
        for status in [
            PaymentStatus::Authorized,
            PaymentStatus::Pending,
            PaymentStatus::Failed,
            PaymentStatus::Voided,
        ] {
            let mut ledger = ledger_with_invoice(10_000, "invoice-1");
            let before = balances(&ledger);
            assert_eq!(
                PaymentLedgerBridge::new().apply(
                    &payment_event(
                        status,
                        "event-inconsistent",
                        "payment-1",
                        "invoice-1",
                        10_000,
                        Some(1_700_000_000_500),
                    ),
                    &accounts(),
                    &mut ledger,
                ),
                Err(PaymentError::InconsistentCaptureState {
                    status,
                    amount_captured: 10_000,
                    has_captured_at: true,
                })
            );
            assert_eq!(ledger.entry_count(), 1);
            assert_eq!(balances(&ledger), before);
        }
    }

    #[test]
    fn partial_and_post_capture_reversal_states_are_unsupported() {
        for status in [
            PaymentStatus::PartialCharged,
            PaymentStatus::AutoRefunded,
            PaymentStatus::VoidedPostCharge,
        ] {
            let mut ledger = ledger_with_invoice(10_000, "invoice-1");
            let before = balances(&ledger);
            assert_eq!(
                PaymentLedgerBridge::new().apply(
                    &payment_event(
                        status,
                        "event-unsupported",
                        "payment-1",
                        "invoice-1",
                        10_000,
                        Some(1_700_000_000_500),
                    ),
                    &accounts(),
                    &mut ledger,
                ),
                Err(PaymentError::UnsupportedPaymentStatus(status))
            );
            assert_eq!(ledger.entry_count(), 1);
            assert_eq!(balances(&ledger), before);
        }
    }

    #[test]
    fn charged_payment_requires_positive_amount_and_timestamp() {
        for amount in [0, -1] {
            let mut ledger = ledger_with_invoice(10_000, "invoice-1");
            assert_eq!(
                PaymentLedgerBridge::new().apply(
                    &payment_event(
                        PaymentStatus::Charged,
                        "event-invalid-amount",
                        "payment-invalid",
                        "invoice-1",
                        amount,
                        Some(1_700_000_000_500),
                    ),
                    &accounts(),
                    &mut ledger,
                ),
                Err(PaymentError::InvalidCapturedAmount(amount))
            );
            assert_eq!(ledger.entry_count(), 1);
        }

        let mut ledger = ledger_with_invoice(10_000, "invoice-1");
        assert_eq!(
            PaymentLedgerBridge::new().apply(
                &payment_event(
                    PaymentStatus::Charged,
                    "event-no-time",
                    "payment-no-time",
                    "invoice-1",
                    10_000,
                    None,
                ),
                &accounts(),
                &mut ledger,
            ),
            Err(PaymentError::MissingCapturedAt)
        );
        assert_eq!(ledger.entry_count(), 1);
    }

    #[test]
    fn capture_cannot_precede_invoice_finalization() {
        let mut ledger = ledger_with_invoice(10_000, "invoice-1");
        let before = balances(&ledger);
        assert_eq!(
            PaymentLedgerBridge::new().apply(
                &payment_event(
                    PaymentStatus::Charged,
                    "event-too-early",
                    "payment-too-early",
                    "invoice-1",
                    10_000,
                    Some(1_699_999_999_999),
                ),
                &accounts(),
                &mut ledger,
            ),
            Err(PaymentError::CapturePrecedesInvoiceFinalization {
                captured_at: 1_699_999_999_999,
                invoice_finalized_at: 1_700_000_000_000,
            })
        );
        assert_eq!(ledger.entry_count(), 1);
        assert_eq!(balances(&ledger), before);
    }

    #[test]
    fn exact_source_event_replay_has_zero_duplicate_effect() {
        let mut ledger = ledger_with_invoice(10_000, "invoice-1");
        let payment = charged_event("payment-event-1", "payment-1", "invoice-1", 10_000);
        let bridge = PaymentLedgerBridge::new();
        let first = bridge.apply(&payment, &accounts(), &mut ledger).unwrap();
        let second = bridge.apply(&payment, &accounts(), &mut ledger).unwrap();
        assert!(matches!(first, PaymentApplyOutcome::Committed { .. }));
        assert!(matches!(second, PaymentApplyOutcome::Replayed { .. }));
        assert_eq!(ledger.entry_count(), 2);
        let (receivable, clearing) = balances(&ledger);
        assert_eq!(receivable.credits(), 10_000);
        assert_eq!(clearing.debits(), 10_000);
    }

    #[test]
    fn conflicting_source_event_reuse_fails_closed() {
        let mut ledger = ledger_with_invoice(10_000, "invoice-1");
        let bridge = PaymentLedgerBridge::new();
        bridge
            .apply(
                &charged_event("payment-event-1", "payment-1", "invoice-1", 10_000),
                &accounts(),
                &mut ledger,
            )
            .unwrap();
        let before = balances(&ledger);

        let result = bridge.apply(
            &charged_event("payment-event-1", "payment-2", "invoice-1", 10_000),
            &accounts(),
            &mut ledger,
        );
        assert!(matches!(
            result,
            Err(PaymentError::LedgerCommit(
                LedgerStateError::IdempotencyConflict(_)
            ))
        ));
        assert_eq!(ledger.entry_count(), 2);
        assert_eq!(balances(&ledger), before);
    }

    #[test]
    fn second_full_payment_for_same_invoice_is_rejected() {
        let mut ledger = ledger_with_invoice(10_000, "invoice-1");
        let bridge = PaymentLedgerBridge::new();
        bridge
            .apply(
                &charged_event("payment-event-1", "payment-1", "invoice-1", 10_000),
                &accounts(),
                &mut ledger,
            )
            .unwrap();
        let before = balances(&ledger);

        let result = bridge.apply(
            &charged_event("payment-event-2", "payment-2", "invoice-1", 10_000),
            &accounts(),
            &mut ledger,
        );
        assert_eq!(
            result,
            Err(PaymentError::LedgerCommit(
                LedgerStateError::BusinessKeyConflict(
                    "payments:invoice:invoice-1:full-capture".to_owned()
                )
            ))
        );
        assert_eq!(ledger.entry_count(), 2);
        assert_eq!(balances(&ledger), before);
    }

    #[test]
    fn same_payment_with_new_source_event_cannot_create_second_history() {
        let mut ledger = ledger_with_invoice(10_000, "invoice-1");
        let second_invoice = InvoiceEvent::new(
            BillingEventId::new("billing-event-invoice-2").unwrap(),
            scope("org-1"),
            invoice_id("invoice-2"),
            BillingCustomerId::new("customer-1").unwrap(),
            InvoiceStatus::Finalized,
            usd(),
            10_000,
            10_000,
            0,
            Some(1_700_000_000_000),
            1_700_000_000_100,
        );
        let billing_accounts =
            BillingLedgerAccounts::new(account_id("accounts-receivable"), account_id("revenue"))
                .unwrap();
        BillingLedgerBridge::new()
            .apply(&second_invoice, &billing_accounts, &mut ledger)
            .unwrap();

        let bridge = PaymentLedgerBridge::new();
        bridge
            .apply(
                &charged_event("payment-event-1", "payment-1", "invoice-1", 10_000),
                &accounts(),
                &mut ledger,
            )
            .unwrap();
        let before = balances(&ledger);

        let result = bridge.apply(
            &charged_event("payment-event-2", "payment-1", "invoice-2", 10_000),
            &accounts(),
            &mut ledger,
        );
        assert!(matches!(
            result,
            Err(PaymentError::LedgerCommit(
                LedgerStateError::DuplicateEntryId(_)
            ))
        ));
        assert_eq!(ledger.entry_count(), 3);
        assert_eq!(balances(&ledger), before);
    }

    #[test]
    fn payment_journal_entry_mapping_is_deterministic() {
        let payment = PaymentId::new("payment-42").unwrap();
        let first = journal_entry_id_for_payment(&payment).unwrap();
        let second = journal_entry_id_for_payment(&payment).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.as_str(), "payments:payment:payment-42:charged");
    }

    #[test]
    fn payment_identities_are_typed_and_empty_values_are_rejected() {
        assert_eq!(
            PaymentEventId::new("   "),
            Err(PaymentError::EmptyIdentifier("payment_event_id"))
        );
        assert_eq!(
            PaymentId::new(""),
            Err(PaymentError::EmptyIdentifier("payment_id"))
        );
        assert_eq!(
            ConnectorTransactionId::new("\t"),
            Err(PaymentError::EmptyIdentifier("connector_transaction_id"))
        );
        assert!(BillingInvoiceId::new("invoice-1").is_ok());
        assert!(PaymentId::new("payment-1").is_ok());
    }

    #[test]
    fn posting_accounts_must_be_distinct() {
        let id = account_id("same");
        assert_eq!(
            PaymentLedgerAccounts::new(id.clone(), id.clone()),
            Err(PaymentError::SamePostingAccount(id))
        );
    }

    fn ledger_with_custom_clearing(
        kind: AccountKind,
        clearing_scope: &str,
        clearing_currency: Currency,
    ) -> Ledger {
        let mut ledger = Ledger::new();
        register_account(
            &mut ledger,
            "accounts-receivable",
            "org-1",
            AccountKind::Asset,
            usd(),
        );
        register_account(&mut ledger, "revenue", "org-1", AccountKind::Revenue, usd());
        let billing_event = InvoiceEvent::new(
            BillingEventId::new("billing-event-custom").unwrap(),
            scope("org-1"),
            invoice_id("invoice-1"),
            BillingCustomerId::new("customer-1").unwrap(),
            InvoiceStatus::Finalized,
            usd(),
            10_000,
            10_000,
            0,
            Some(1_700_000_000_000),
            1_700_000_000_100,
        );
        let billing_accounts =
            BillingLedgerAccounts::new(account_id("accounts-receivable"), account_id("revenue"))
                .unwrap();
        BillingLedgerBridge::new()
            .apply(&billing_event, &billing_accounts, &mut ledger)
            .unwrap();
        register_account(
            &mut ledger,
            "processor-clearing",
            clearing_scope,
            kind,
            clearing_currency,
        );
        ledger
    }

    #[test]
    fn unknown_payment_account_is_rejected_before_commit() {
        let mut ledger = ledger_with_invoice(10_000, "invoice-1");
        let custom_accounts = PaymentLedgerAccounts::new(
            account_id("accounts-receivable"),
            account_id("missing-clearing"),
        )
        .unwrap();
        let before = balances(&ledger);

        assert_eq!(
            PaymentLedgerBridge::new().apply(
                &charged_event("event-missing-account", "payment-1", "invoice-1", 10_000),
                &custom_accounts,
                &mut ledger,
            ),
            Err(PaymentError::UnknownLedgerAccount(account_id(
                "missing-clearing"
            )))
        );
        assert_eq!(ledger.entry_count(), 1);
        assert_eq!(balances(&ledger), before);
    }

    #[test]
    fn wrong_clearing_account_kind_is_rejected_before_commit() {
        let mut ledger = ledger_with_custom_clearing(AccountKind::Revenue, "org-1", usd());
        let before = balances(&ledger);
        assert_eq!(
            PaymentLedgerBridge::new().apply(
                &charged_event("event-kind", "payment-1", "invoice-1", 10_000),
                &accounts(),
                &mut ledger,
            ),
            Err(PaymentError::AccountKindMismatch {
                account_id: account_id("processor-clearing"),
                expected: AccountKind::Asset,
                actual: AccountKind::Revenue,
            })
        );
        assert_eq!(ledger.entry_count(), 1);
        assert_eq!(balances(&ledger), before);
    }

    #[test]
    fn cross_scope_clearing_account_is_rejected_before_commit() {
        let mut ledger = ledger_with_custom_clearing(AccountKind::Asset, "org-2", usd());
        let before = balances(&ledger);
        assert_eq!(
            PaymentLedgerBridge::new().apply(
                &charged_event("event-scope", "payment-1", "invoice-1", 10_000),
                &accounts(),
                &mut ledger,
            ),
            Err(PaymentError::AccountScopeMismatch {
                account_id: account_id("processor-clearing"),
                expected: scope("org-1"),
                actual: scope("org-2"),
            })
        );
        assert_eq!(ledger.entry_count(), 1);
        assert_eq!(balances(&ledger), before);
    }

    #[test]
    fn currency_mismatched_clearing_account_is_rejected_before_commit() {
        let mut ledger = ledger_with_custom_clearing(AccountKind::Asset, "org-1", eur());
        let before = balances(&ledger);
        assert_eq!(
            PaymentLedgerBridge::new().apply(
                &charged_event("event-currency", "payment-1", "invoice-1", 10_000),
                &accounts(),
                &mut ledger,
            ),
            Err(PaymentError::AccountCurrencyMismatch {
                account_id: account_id("processor-clearing"),
                expected: usd(),
                actual: eur(),
            })
        );
        assert_eq!(ledger.entry_count(), 1);
        assert_eq!(balances(&ledger), before);
    }

    #[test]
    fn configured_receivable_must_match_invoice_receivable_posting() {
        let mut ledger = ledger_with_invoice(10_000, "invoice-1");
        register_account(
            &mut ledger,
            "alternate-receivable",
            "org-1",
            AccountKind::Asset,
            usd(),
        );
        let custom_accounts = PaymentLedgerAccounts::new(
            account_id("alternate-receivable"),
            account_id("processor-clearing"),
        )
        .unwrap();
        let before_count = ledger.entry_count();

        assert_eq!(
            PaymentLedgerBridge::new().apply(
                &charged_event("event-alt-ar", "payment-1", "invoice-1", 10_000),
                &custom_accounts,
                &mut ledger,
            ),
            Err(PaymentError::InvoiceReceivablePostingMissing {
                invoice_entry_id: journal_entry_id_for_invoice(&invoice_id("invoice-1")).unwrap(),
                account_id: account_id("alternate-receivable"),
            })
        );
        assert_eq!(ledger.entry_count(), before_count);
        assert_eq!(
            ledger.balance(&account_id("processor-clearing")).unwrap(),
            AccountBalance::default()
        );
        assert_eq!(
            ledger.balance(&account_id("alternate-receivable")).unwrap(),
            AccountBalance::default()
        );
    }
}
