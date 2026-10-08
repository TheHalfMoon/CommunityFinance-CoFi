use std::error::Error;
use std::fmt::{Display, Formatter};

use cofi_ledger::{
    AccountId, AccountKind, CommitOutcome, Currency, EntryMetadata, JournalEntry, JournalEntryId,
    Ledger, LedgerError, LedgerScopeId, LedgerStateError, Posting, Side,
};

use crate::{PaymentId, journal_entry_id_for_payment};

macro_rules! payout_id {
    ($name:ident, $label:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, PayoutError> {
                let value = value.into();
                if value.trim().is_empty() {
                    return Err(PayoutError::EmptyIdentifier($label));
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

payout_id!(ProcessorPayoutEventId, "processor_payout_event_id");
payout_id!(ProcessorPayoutId, "processor_payout_id");
payout_id!(BankTransactionReference, "bank_transaction_reference");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessorPayoutEvent {
    source_event_id: ProcessorPayoutEventId,
    organization_scope: LedgerScopeId,
    payment_id: PaymentId,
    payout_id: ProcessorPayoutId,
    bank_transaction_reference: BankTransactionReference,
    currency: Currency,
    gross_amount_minor: i128,
    processor_fee_minor: i128,
    net_amount_minor: i128,
    paid_at_unix_ms: i64,
    observed_at_unix_ms: i64,
}

impl ProcessorPayoutEvent {
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        source_event_id: ProcessorPayoutEventId,
        organization_scope: LedgerScopeId,
        payment_id: PaymentId,
        payout_id: ProcessorPayoutId,
        bank_transaction_reference: BankTransactionReference,
        currency: Currency,
        gross_amount_minor: i128,
        processor_fee_minor: i128,
        net_amount_minor: i128,
        paid_at_unix_ms: i64,
        observed_at_unix_ms: i64,
    ) -> Self {
        Self {
            source_event_id,
            organization_scope,
            payment_id,
            payout_id,
            bank_transaction_reference,
            currency,
            gross_amount_minor,
            processor_fee_minor,
            net_amount_minor,
            paid_at_unix_ms,
            observed_at_unix_ms,
        }
    }

    #[must_use]
    pub fn payment_id(&self) -> &PaymentId {
        &self.payment_id
    }

    #[must_use]
    pub fn payout_id(&self) -> &ProcessorPayoutId {
        &self.payout_id
    }

    /// Original payout source event and immutable journal inputs.
    #[must_use]
    pub const fn source_event_id(&self) -> &ProcessorPayoutEventId {
        &self.source_event_id
    }
    #[must_use]
    pub const fn bank_transaction_reference(&self) -> &BankTransactionReference {
        &self.bank_transaction_reference
    }
    #[must_use]
    pub const fn gross_amount_minor(&self) -> i128 {
        self.gross_amount_minor
    }
    #[must_use]
    pub const fn processor_fee_minor(&self) -> i128 {
        self.processor_fee_minor
    }
    #[must_use]
    pub const fn net_amount_minor(&self) -> i128 {
        self.net_amount_minor
    }
    #[must_use]
    pub const fn paid_at_unix_ms(&self) -> i64 {
        self.paid_at_unix_ms
    }
    #[must_use]
    pub const fn observed_at_unix_ms(&self) -> i64 {
        self.observed_at_unix_ms
    }
    #[must_use]
    pub fn organization_scope(&self) -> &LedgerScopeId {
        &self.organization_scope
    }

    #[must_use]
    pub const fn currency(&self) -> Currency {
        self.currency
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayoutLedgerAccounts {
    processor_clearing: AccountId,
    bank_cash: AccountId,
    processor_fee_expense: Option<AccountId>,
}

impl PayoutLedgerAccounts {
    pub fn new(
        processor_clearing: AccountId,
        bank_cash: AccountId,
        processor_fee_expense: Option<AccountId>,
    ) -> Result<Self, PayoutError> {
        if processor_clearing == bank_cash {
            return Err(PayoutError::SamePostingAccount(processor_clearing));
        }
        if let Some(fee) = &processor_fee_expense {
            if fee == &processor_clearing || fee == &bank_cash {
                return Err(PayoutError::DuplicatePostingAccount(fee.clone()));
            }
        }
        Ok(Self {
            processor_clearing,
            bank_cash,
            processor_fee_expense,
        })
    }

    #[must_use]
    pub fn processor_clearing(&self) -> &AccountId {
        &self.processor_clearing
    }

    #[must_use]
    pub fn bank_cash(&self) -> &AccountId {
        &self.bank_cash
    }

    #[must_use]
    pub fn processor_fee_expense(&self) -> Option<&AccountId> {
        self.processor_fee_expense.as_ref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PayoutApplyOutcome {
    Committed { journal_entry_id: JournalEntryId },
    Replayed { journal_entry_id: JournalEntryId },
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PayoutLedgerBridge;

impl PayoutLedgerBridge {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    pub fn apply(
        &self,
        event: &ProcessorPayoutEvent,
        accounts: &PayoutLedgerAccounts,
        ledger: &mut Ledger,
    ) -> Result<PayoutApplyOutcome, PayoutError> {
        validate_amounts(event)?;
        validate_account(
            ledger,
            accounts.processor_clearing(),
            event.organization_scope(),
            event.currency(),
            AccountKind::Asset,
        )?;
        validate_account(
            ledger,
            accounts.bank_cash(),
            event.organization_scope(),
            event.currency(),
            AccountKind::Asset,
        )?;

        let fee_account = if event.processor_fee_minor > 0 {
            let account = accounts
                .processor_fee_expense()
                .ok_or(PayoutError::MissingFeeExpenseAccount)?;
            validate_account(
                ledger,
                account,
                event.organization_scope(),
                event.currency(),
                AccountKind::Expense,
            )?;
            Some(account.clone())
        } else {
            None
        };

        let payment_entry_id = journal_entry_id_for_payment(event.payment_id())
            .map_err(PayoutError::PaymentReference)?;
        let payment_entry = ledger
            .entry(&payment_entry_id)
            .ok_or_else(|| PayoutError::MissingPaymentJournalEntry(payment_entry_id.clone()))?;
        let clearing_amount = payment_clearing_amount(
            payment_entry,
            &payment_entry_id,
            accounts.processor_clearing(),
            event.currency(),
        )?;

        let gross = event.gross_amount_minor as u128;
        if clearing_amount != gross {
            return Err(PayoutError::GrossAmountMismatch {
                expected: clearing_amount,
                actual: gross,
            });
        }
        if event.paid_at_unix_ms < payment_entry.effective_at_unix_ms() {
            return Err(PayoutError::PayoutPrecedesCapture {
                paid_at: event.paid_at_unix_ms,
                captured_at: payment_entry.effective_at_unix_ms(),
            });
        }

        let entry_id = journal_entry_id_for_payout(event.payout_id())?;
        let metadata = EntryMetadata::new(
            Some(payout_payload_correlation(event)),
            Some(event.source_event_id.as_str().to_owned()),
        )
        .and_then(|metadata| {
            metadata.with_business_key(Some(payment_payout_business_key(event.payment_id())))
        })
        .map_err(PayoutError::LedgerBuild)?;

        let mut postings = vec![
            Posting::new(
                accounts.bank_cash().clone(),
                event.currency(),
                Side::Debit,
                event.net_amount_minor,
            )
            .map_err(PayoutError::LedgerBuild)?,
        ];
        if let Some(fee_account) = fee_account {
            postings.push(
                Posting::new(
                    fee_account,
                    event.currency(),
                    Side::Debit,
                    event.processor_fee_minor,
                )
                .map_err(PayoutError::LedgerBuild)?,
            );
        }
        postings.push(
            Posting::new(
                accounts.processor_clearing().clone(),
                event.currency(),
                Side::Credit,
                event.gross_amount_minor,
            )
            .map_err(PayoutError::LedgerBuild)?,
        );

        let entry = JournalEntry::new(
            entry_id.clone(),
            postings,
            event.paid_at_unix_ms,
            event.observed_at_unix_ms,
            metadata,
        )
        .map_err(PayoutError::LedgerBuild)?;

        match ledger.commit(entry).map_err(PayoutError::LedgerCommit)? {
            CommitOutcome::Committed => Ok(PayoutApplyOutcome::Committed {
                journal_entry_id: entry_id,
            }),
            CommitOutcome::Replayed => Ok(PayoutApplyOutcome::Replayed {
                journal_entry_id: entry_id,
            }),
        }
    }
}

pub fn journal_entry_id_for_payout(
    payout_id: &ProcessorPayoutId,
) -> Result<JournalEntryId, PayoutError> {
    JournalEntryId::new(format!("payments:payout:{}:reconciled", payout_id.as_str()))
        .map_err(PayoutError::LedgerBuild)
}

fn payment_payout_business_key(payment_id: &PaymentId) -> String {
    format!("payments:payment:{}:full-payout", payment_id.as_str())
}
fn payout_payload_correlation(event: &ProcessorPayoutEvent) -> String {
    format!(
        "payout:{}:{}:{}:{}:{}:{}:{}:{}:{}",
        event.payment_id.as_str(),
        event.payout_id.as_str(),
        event.bank_transaction_reference.as_str(),
        event.organization_scope.as_str(),
        event.currency.code(),
        event.gross_amount_minor,
        event.processor_fee_minor,
        event.net_amount_minor,
        event.paid_at_unix_ms,
    )
}

fn validate_amounts(event: &ProcessorPayoutEvent) -> Result<(), PayoutError> {
    if event.gross_amount_minor <= 0 {
        return Err(PayoutError::InvalidGrossAmount(event.gross_amount_minor));
    }
    if event.processor_fee_minor < 0 {
        return Err(PayoutError::InvalidProcessorFee(event.processor_fee_minor));
    }
    if event.net_amount_minor <= 0 {
        return Err(PayoutError::InvalidNetAmount(event.net_amount_minor));
    }
    let recomposed = event
        .net_amount_minor
        .checked_add(event.processor_fee_minor)
        .ok_or(PayoutError::AmountArithmeticOverflow)?;
    if recomposed != event.gross_amount_minor {
        return Err(PayoutError::GrossNetFeeMismatch {
            gross: event.gross_amount_minor,
            net: event.net_amount_minor,
            fee: event.processor_fee_minor,
        });
    }
    Ok(())
}

fn validate_account(
    ledger: &Ledger,
    account_id: &AccountId,
    expected_scope: &LedgerScopeId,
    expected_currency: Currency,
    expected_kind: AccountKind,
) -> Result<(), PayoutError> {
    let account = ledger
        .account(account_id)
        .ok_or_else(|| PayoutError::UnknownLedgerAccount(account_id.clone()))?;
    if account.kind() != expected_kind {
        return Err(PayoutError::AccountKindMismatch {
            account_id: account_id.clone(),
            expected: expected_kind,
            actual: account.kind(),
        });
    }
    if account.scope_id() != expected_scope {
        return Err(PayoutError::AccountScopeMismatch {
            account_id: account_id.clone(),
            expected: expected_scope.clone(),
            actual: account.scope_id().clone(),
        });
    }
    if account.currency() != expected_currency {
        return Err(PayoutError::AccountCurrencyMismatch {
            account_id: account_id.clone(),
            expected: expected_currency,
            actual: account.currency(),
        });
    }
    Ok(())
}

fn payment_clearing_amount(
    entry: &JournalEntry,
    payment_entry_id: &JournalEntryId,
    processor_clearing: &AccountId,
    expected_currency: Currency,
) -> Result<u128, PayoutError> {
    let mut matches = entry
        .postings()
        .iter()
        .filter(|posting| posting.account_id() == processor_clearing);
    let posting = matches
        .next()
        .ok_or_else(|| PayoutError::PaymentClearingPostingMissing {
            payment_entry_id: payment_entry_id.clone(),
            account_id: processor_clearing.clone(),
        })?;
    if matches.next().is_some()
        || posting.side() != Side::Debit
        || posting.currency() != expected_currency
    {
        return Err(PayoutError::PaymentClearingPostingInvalid {
            payment_entry_id: payment_entry_id.clone(),
            account_id: processor_clearing.clone(),
        });
    }
    Ok(posting.amount().value())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PayoutError {
    EmptyIdentifier(&'static str),
    SamePostingAccount(AccountId),
    DuplicatePostingAccount(AccountId),
    InvalidGrossAmount(i128),
    InvalidProcessorFee(i128),
    InvalidNetAmount(i128),
    AmountArithmeticOverflow,
    GrossNetFeeMismatch {
        gross: i128,
        net: i128,
        fee: i128,
    },
    MissingFeeExpenseAccount,
    PaymentReference(crate::PaymentError),
    MissingPaymentJournalEntry(JournalEntryId),
    PaymentClearingPostingMissing {
        payment_entry_id: JournalEntryId,
        account_id: AccountId,
    },
    PaymentClearingPostingInvalid {
        payment_entry_id: JournalEntryId,
        account_id: AccountId,
    },
    GrossAmountMismatch {
        expected: u128,
        actual: u128,
    },
    PayoutPrecedesCapture {
        paid_at: i64,
        captured_at: i64,
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

impl Display for PayoutError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyIdentifier(name) => write!(f, "{name} must not be empty"),
            Self::SamePostingAccount(id) => write!(
                f,
                "processor clearing and bank accounts must be distinct: {}",
                id.as_str()
            ),
            Self::DuplicatePostingAccount(id) => write!(
                f,
                "payout posting accounts must be distinct: {}",
                id.as_str()
            ),
            Self::InvalidGrossAmount(value) => {
                write!(f, "payout gross amount must be positive: {value}")
            }
            Self::InvalidProcessorFee(value) => {
                write!(f, "processor fee must be non-negative: {value}")
            }
            Self::InvalidNetAmount(value) => {
                write!(f, "payout net amount must be positive: {value}")
            }
            Self::AmountArithmeticOverflow => f.write_str("payout net + fee arithmetic overflow"),
            Self::GrossNetFeeMismatch { gross, net, fee } => write!(
                f,
                "payout gross must equal net + fee: gross={gross}, net={net}, fee={fee}"
            ),
            Self::MissingFeeExpenseAccount => {
                f.write_str("processor fee expense account is required when fee is positive")
            }
            Self::PaymentReference(error) => write!(f, "invalid payment reference: {error}"),
            Self::MissingPaymentJournalEntry(id) => write!(
                f,
                "referenced payment has no canonical P05 journal entry: {}",
                id.as_str()
            ),
            Self::PaymentClearingPostingMissing {
                payment_entry_id,
                account_id,
            } => write!(
                f,
                "payment entry {} does not debit processor clearing account {}",
                payment_entry_id.as_str(),
                account_id.as_str()
            ),
            Self::PaymentClearingPostingInvalid {
                payment_entry_id,
                account_id,
            } => write!(
                f,
                "payment entry {} has invalid processor clearing posting for account {}",
                payment_entry_id.as_str(),
                account_id.as_str()
            ),
            Self::GrossAmountMismatch { expected, actual } => write!(
                f,
                "P06 supports full payout only: expected processor clearing {expected}, gross {actual}"
            ),
            Self::PayoutPrecedesCapture {
                paid_at,
                captured_at,
            } => write!(
                f,
                "processor payout precedes payment capture: paid_at={paid_at}, captured_at={captured_at}"
            ),
            Self::UnknownLedgerAccount(id) => write!(
                f,
                "payout ledger account is not registered: {}",
                id.as_str()
            ),
            Self::AccountKindMismatch {
                account_id,
                expected,
                actual,
            } => write!(
                f,
                "payout ledger account {} has kind {actual:?}; expected {expected:?}",
                account_id.as_str()
            ),
            Self::AccountScopeMismatch {
                account_id,
                expected,
                actual,
            } => write!(
                f,
                "payout ledger account {} has scope {}; expected {}",
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
                "payout ledger account {} has currency {actual}; expected {expected}",
                account_id.as_str()
            ),
            Self::LedgerBuild(error) => write!(f, "failed to build payout journal entry: {error}"),
            Self::LedgerCommit(error) => {
                write!(f, "failed to commit payout journal entry: {error}")
            }
        }
    }
}

impl Error for PayoutError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::PaymentReference(error) => Some(error),
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
    use crate::{
        ConnectorTransactionId, PaymentApplyOutcome, PaymentEvent, PaymentEventId,
        PaymentLedgerAccounts, PaymentLedgerBridge, PaymentStatus,
    };
    use cofi_billing::{
        BillingCustomerId, BillingEventId, BillingInvoiceId, BillingLedgerAccounts,
        BillingLedgerBridge, InvoiceEvent, InvoiceStatus,
    };
    use cofi_ledger::Account;

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

    fn payment_id(value: &str) -> PaymentId {
        PaymentId::new(value).unwrap()
    }

    fn payout_id(value: &str) -> ProcessorPayoutId {
        ProcessorPayoutId::new(value).unwrap()
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

    fn ledger_with_payment(amount: i128, payment: &str) -> Ledger {
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
        register_account(&mut ledger, "bank-cash", "org-1", AccountKind::Asset, usd());
        register_account(
            &mut ledger,
            "processor-fee-expense",
            "org-1",
            AccountKind::Expense,
            usd(),
        );

        let invoice_id = BillingInvoiceId::new(format!("invoice-{payment}")).unwrap();
        let billing_event = InvoiceEvent::new(
            BillingEventId::new(format!("billing-event-{payment}")).unwrap(),
            scope("org-1"),
            invoice_id.clone(),
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

        let payment_event = PaymentEvent::new(
            PaymentEventId::new(format!("payment-event-{payment}")).unwrap(),
            scope("org-1"),
            invoice_id,
            payment_id(payment),
            ConnectorTransactionId::new(format!("connector-{payment}")).unwrap(),
            PaymentStatus::Charged,
            usd(),
            amount,
            Some(1_700_000_000_200),
            1_700_000_000_300,
        );
        let payment_accounts = PaymentLedgerAccounts::new(
            account_id("accounts-receivable"),
            account_id("processor-clearing"),
        )
        .unwrap();
        assert!(matches!(
            PaymentLedgerBridge::new()
                .apply(&payment_event, &payment_accounts, &mut ledger)
                .unwrap(),
            PaymentApplyOutcome::Committed { .. }
        ));
        ledger
    }

    fn payout_event(
        source: &str,
        payment: &str,
        payout: &str,
        gross: i128,
        fee: i128,
        net: i128,
    ) -> ProcessorPayoutEvent {
        ProcessorPayoutEvent::new(
            ProcessorPayoutEventId::new(source).unwrap(),
            scope("org-1"),
            payment_id(payment),
            payout_id(payout),
            BankTransactionReference::new(format!("bank-{payout}")).unwrap(),
            usd(),
            gross,
            fee,
            net,
            1_700_000_000_400,
            1_700_000_000_500,
        )
    }

    fn no_fee_accounts() -> PayoutLedgerAccounts {
        PayoutLedgerAccounts::new(
            account_id("processor-clearing"),
            account_id("bank-cash"),
            None,
        )
        .unwrap()
    }

    fn fee_accounts() -> PayoutLedgerAccounts {
        PayoutLedgerAccounts::new(
            account_id("processor-clearing"),
            account_id("bank-cash"),
            Some(account_id("processor-fee-expense")),
        )
        .unwrap()
    }

    #[test]
    fn no_fee_payout_moves_clearing_to_bank() {
        let mut ledger = ledger_with_payment(1_000, "payment-1");
        let event = payout_event("payout-event-1", "payment-1", "payout-1", 1_000, 0, 1_000);
        let outcome = PayoutLedgerBridge::new()
            .apply(&event, &no_fee_accounts(), &mut ledger)
            .unwrap();
        assert!(matches!(outcome, PayoutApplyOutcome::Committed { .. }));
        let journal_entry_id = journal_entry_id_for_payout(&payout_id("payout-1")).unwrap();
        let entry = ledger.entry(&journal_entry_id).unwrap();
        assert_eq!(entry.postings().len(), 2);
        assert!(entry.postings().iter().any(|posting| {
            posting.account_id() == &account_id("bank-cash")
                && posting.side() == Side::Debit
                && posting.amount().value() == 1_000
        }));
    }

    #[test]
    fn fee_payout_posts_bank_fee_and_clearing() {
        let mut ledger = ledger_with_payment(1_000, "payment-2");
        let event = payout_event("payout-event-2", "payment-2", "payout-2", 1_000, 100, 900);
        let outcome = PayoutLedgerBridge::new()
            .apply(&event, &fee_accounts(), &mut ledger)
            .unwrap();
        assert!(matches!(outcome, PayoutApplyOutcome::Committed { .. }));
        let journal_entry_id = journal_entry_id_for_payout(&payout_id("payout-2")).unwrap();
        let entry = ledger.entry(&journal_entry_id).unwrap();
        assert_eq!(entry.postings().len(), 3);
        assert!(entry.postings().iter().any(|posting| {
            posting.account_id() == &account_id("processor-fee-expense")
                && posting.side() == Side::Debit
                && posting.amount().value() == 100
        }));
        assert!(entry.postings().iter().any(|posting| {
            posting.account_id() == &account_id("processor-clearing")
                && posting.side() == Side::Credit
                && posting.amount().value() == 1_000
        }));
    }

    #[test]
    fn payout_requires_canonical_payment() {
        let mut ledger = ledger_with_payment(1_000, "payment-3");
        let event = payout_event(
            "payout-event-3",
            "missing-payment",
            "payout-3",
            1_000,
            0,
            1_000,
        );
        let error = PayoutLedgerBridge::new()
            .apply(&event, &no_fee_accounts(), &mut ledger)
            .unwrap_err();
        assert!(matches!(error, PayoutError::MissingPaymentJournalEntry(_)));
    }

    #[test]
    fn gross_must_match_payment_clearing() {
        let mut ledger = ledger_with_payment(1_000, "payment-4");
        let event = payout_event("payout-event-4", "payment-4", "payout-4", 900, 0, 900);
        let error = PayoutLedgerBridge::new()
            .apply(&event, &no_fee_accounts(), &mut ledger)
            .unwrap_err();
        assert!(matches!(
            error,
            PayoutError::GrossAmountMismatch {
                expected: 1_000,
                actual: 900
            }
        ));
    }

    #[test]
    fn gross_must_equal_net_plus_fee() {
        let mut ledger = ledger_with_payment(1_000, "payment-5");
        let event = payout_event("payout-event-5", "payment-5", "payout-5", 1_000, 100, 850);
        let before = ledger.entry_count();
        let error = PayoutLedgerBridge::new()
            .apply(&event, &fee_accounts(), &mut ledger)
            .unwrap_err();
        assert!(matches!(error, PayoutError::GrossNetFeeMismatch { .. }));
        assert_eq!(ledger.entry_count(), before);
    }

    #[test]
    fn negative_fee_is_rejected() {
        let mut ledger = ledger_with_payment(1_000, "payment-6");
        let event = payout_event("payout-event-6", "payment-6", "payout-6", 1_000, -1, 1_001);
        let error = PayoutLedgerBridge::new()
            .apply(&event, &fee_accounts(), &mut ledger)
            .unwrap_err();
        assert_eq!(error, PayoutError::InvalidProcessorFee(-1));
    }

    #[test]
    fn amount_addition_overflow_is_rejected() {
        let mut ledger = ledger_with_payment(1_000, "payment-7");
        let event = payout_event(
            "payout-event-7",
            "payment-7",
            "payout-7",
            i128::MAX,
            1,
            i128::MAX,
        );
        let error = PayoutLedgerBridge::new()
            .apply(&event, &fee_accounts(), &mut ledger)
            .unwrap_err();
        assert_eq!(error, PayoutError::AmountArithmeticOverflow);
    }

    #[test]
    fn payout_cannot_precede_capture() {
        let mut ledger = ledger_with_payment(1_000, "payment-8");
        let mut event = payout_event("payout-event-8", "payment-8", "payout-8", 1_000, 0, 1_000);
        event.paid_at_unix_ms = 1_700_000_000_150;
        let error = PayoutLedgerBridge::new()
            .apply(&event, &no_fee_accounts(), &mut ledger)
            .unwrap_err();
        assert!(matches!(error, PayoutError::PayoutPrecedesCapture { .. }));
    }

    #[test]
    fn positive_fee_requires_expense_account() {
        let mut ledger = ledger_with_payment(1_000, "payment-9");
        let event = payout_event("payout-event-9", "payment-9", "payout-9", 1_000, 100, 900);
        let error = PayoutLedgerBridge::new()
            .apply(&event, &no_fee_accounts(), &mut ledger)
            .unwrap_err();
        assert_eq!(error, PayoutError::MissingFeeExpenseAccount);
    }

    #[test]
    fn payout_accounts_validate_kind_scope_and_currency() {
        let mut ledger = ledger_with_payment(1_000, "payment-10");
        register_account(
            &mut ledger,
            "wrong-kind",
            "org-1",
            AccountKind::Revenue,
            usd(),
        );
        register_account(
            &mut ledger,
            "other-scope",
            "org-2",
            AccountKind::Asset,
            usd(),
        );
        register_account(&mut ledger, "eur-bank", "org-1", AccountKind::Asset, eur());
        let event = payout_event(
            "payout-event-10",
            "payment-10",
            "payout-10",
            1_000,
            0,
            1_000,
        );

        let wrong_kind = PayoutLedgerAccounts::new(
            account_id("processor-clearing"),
            account_id("wrong-kind"),
            None,
        )
        .unwrap();
        assert!(matches!(
            PayoutLedgerBridge::new().apply(&event, &wrong_kind, &mut ledger),
            Err(PayoutError::AccountKindMismatch { .. })
        ));

        let wrong_scope = PayoutLedgerAccounts::new(
            account_id("processor-clearing"),
            account_id("other-scope"),
            None,
        )
        .unwrap();
        assert!(matches!(
            PayoutLedgerBridge::new().apply(&event, &wrong_scope, &mut ledger),
            Err(PayoutError::AccountScopeMismatch { .. })
        ));

        let wrong_currency = PayoutLedgerAccounts::new(
            account_id("processor-clearing"),
            account_id("eur-bank"),
            None,
        )
        .unwrap();
        assert!(matches!(
            PayoutLedgerBridge::new().apply(&event, &wrong_currency, &mut ledger),
            Err(PayoutError::AccountCurrencyMismatch { .. })
        ));
    }

    #[test]
    fn payout_accounts_must_be_distinct() {
        let same = account_id("processor-clearing");
        assert!(matches!(
            PayoutLedgerAccounts::new(same.clone(), same, None),
            Err(PayoutError::SamePostingAccount(_))
        ));
        assert!(matches!(
            PayoutLedgerAccounts::new(
                account_id("processor-clearing"),
                account_id("bank-cash"),
                Some(account_id("bank-cash")),
            ),
            Err(PayoutError::DuplicatePostingAccount(_))
        ));
    }

    #[test]
    fn exact_payout_replay_has_zero_duplicate_effect() {
        let mut ledger = ledger_with_payment(1_000, "payment-11");
        let event = payout_event(
            "payout-event-11",
            "payment-11",
            "payout-11",
            1_000,
            0,
            1_000,
        );
        let bridge = PayoutLedgerBridge::new();
        let first = bridge
            .apply(&event, &no_fee_accounts(), &mut ledger)
            .unwrap();
        let count = ledger.entry_count();
        let second = bridge
            .apply(&event, &no_fee_accounts(), &mut ledger)
            .unwrap();
        assert!(matches!(first, PayoutApplyOutcome::Committed { .. }));
        assert!(matches!(second, PayoutApplyOutcome::Replayed { .. }));
        assert_eq!(ledger.entry_count(), count);
    }

    #[test]
    fn conflicting_source_event_reuse_fails_closed() {
        let mut ledger = ledger_with_payment(1_000, "payment-12");
        let first = payout_event("shared-source", "payment-12", "payout-12", 1_000, 0, 1_000);
        PayoutLedgerBridge::new()
            .apply(&first, &no_fee_accounts(), &mut ledger)
            .unwrap();
        let count = ledger.entry_count();
        let conflicting =
            payout_event("shared-source", "payment-12", "payout-12b", 1_000, 0, 1_000);
        assert!(
            PayoutLedgerBridge::new()
                .apply(&conflicting, &no_fee_accounts(), &mut ledger)
                .is_err()
        );
        assert_eq!(ledger.entry_count(), count);
    }

    #[test]
    fn same_payout_id_cannot_create_second_history() {
        let mut ledger = ledger_with_payment(1_000, "payment-13");
        let first = payout_event("source-13a", "payment-13", "payout-13", 1_000, 0, 1_000);
        PayoutLedgerBridge::new()
            .apply(&first, &no_fee_accounts(), &mut ledger)
            .unwrap();
        let count = ledger.entry_count();
        let second = payout_event("source-13b", "payment-13", "payout-13", 1_000, 0, 1_000);
        assert!(
            PayoutLedgerBridge::new()
                .apply(&second, &no_fee_accounts(), &mut ledger)
                .is_err()
        );
        assert_eq!(ledger.entry_count(), count);
    }

    #[test]
    fn same_payment_cannot_receive_second_full_payout() {
        let mut ledger = ledger_with_payment(1_000, "payment-14");
        let first = payout_event("source-14a", "payment-14", "payout-14a", 1_000, 0, 1_000);
        PayoutLedgerBridge::new()
            .apply(&first, &no_fee_accounts(), &mut ledger)
            .unwrap();
        let count = ledger.entry_count();
        let second = payout_event("source-14b", "payment-14", "payout-14b", 1_000, 0, 1_000);
        assert!(
            PayoutLedgerBridge::new()
                .apply(&second, &no_fee_accounts(), &mut ledger)
                .is_err()
        );
        assert_eq!(ledger.entry_count(), count);
    }

    #[test]
    fn rejected_payout_does_not_reserve_payment_business_key() {
        let mut ledger = ledger_with_payment(1_000, "payment-15");
        let invalid = payout_event("source-15a", "payment-15", "payout-15a", 1_000, 100, 850);
        let before = ledger.entry_count();
        assert!(
            PayoutLedgerBridge::new()
                .apply(&invalid, &fee_accounts(), &mut ledger)
                .is_err()
        );
        assert_eq!(ledger.entry_count(), before);

        let valid = payout_event("source-15b", "payment-15", "payout-15b", 1_000, 100, 900);
        assert!(matches!(
            PayoutLedgerBridge::new()
                .apply(&valid, &fee_accounts(), &mut ledger)
                .unwrap(),
            PayoutApplyOutcome::Committed { .. }
        ));
    }

    #[test]
    fn fee_account_must_be_expense_kind() {
        let mut ledger = ledger_with_payment(1_000, "payment-16");
        let accounts = PayoutLedgerAccounts::new(
            account_id("processor-clearing"),
            account_id("bank-cash"),
            Some(account_id("revenue")),
        )
        .unwrap();
        let event = payout_event("source-16", "payment-16", "payout-16", 1_000, 100, 900);
        assert!(matches!(
            PayoutLedgerBridge::new().apply(&event, &accounts, &mut ledger),
            Err(PayoutError::AccountKindMismatch { .. })
        ));
    }

    #[test]
    fn configured_clearing_must_match_payment_clearing_posting() {
        let mut ledger = ledger_with_payment(1_000, "payment-17");
        register_account(
            &mut ledger,
            "alternate-clearing",
            "org-1",
            AccountKind::Asset,
            usd(),
        );
        let accounts = PayoutLedgerAccounts::new(
            account_id("alternate-clearing"),
            account_id("bank-cash"),
            None,
        )
        .unwrap();
        let event = payout_event("source-17", "payment-17", "payout-17", 1_000, 0, 1_000);
        assert!(matches!(
            PayoutLedgerBridge::new().apply(&event, &accounts, &mut ledger),
            Err(PayoutError::PaymentClearingPostingMissing { .. })
        ));
    }
}
