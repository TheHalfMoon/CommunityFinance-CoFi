use std::error::Error;
use std::fmt::{Display, Formatter};

use cofi_ledger::{
    AccountId, AccountKind, CommitOutcome, Currency, EntryMetadata, JournalEntry, JournalEntryId,
    Ledger, LedgerError, LedgerScopeId, LedgerStateError, Posting, Side,
};

macro_rules! billing_id {
    ($name:ident, $label:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, BillingError> {
                let value = value.into();
                if value.trim().is_empty() {
                    return Err(BillingError::EmptyIdentifier($label));
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
billing_id!(BillingInvoiceId, "billing_invoice_id");
billing_id!(BillingCustomerId, "billing_customer_id");
billing_id!(BillingEventId, "billing_event_id");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvoiceStatus {
    Draft,
    Finalized,
    Void,
    Uncollectible,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvoiceEvent {
    source_event_id: BillingEventId,
    organization_scope: LedgerScopeId,
    invoice_id: BillingInvoiceId,
    customer_id: BillingCustomerId,
    status: InvoiceStatus,
    currency: Currency,
    amount_due_minor: i128,
    total_minor: i128,
    tax_amount_minor: i128,
    finalized_at_unix_ms: Option<i64>,
    observed_at_unix_ms: i64,
}

impl InvoiceEvent {
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        source_event_id: BillingEventId,
        organization_scope: LedgerScopeId,
        invoice_id: BillingInvoiceId,
        customer_id: BillingCustomerId,
        status: InvoiceStatus,
        currency: Currency,
        amount_due_minor: i128,
        total_minor: i128,
        tax_amount_minor: i128,
        finalized_at_unix_ms: Option<i64>,
        observed_at_unix_ms: i64,
    ) -> Self {
        Self {
            source_event_id,
            organization_scope,
            invoice_id,
            customer_id,
            status,
            currency,
            amount_due_minor,
            total_minor,
            tax_amount_minor,
            finalized_at_unix_ms,
            observed_at_unix_ms,
        }
    }

    #[must_use]
    pub fn invoice_id(&self) -> &BillingInvoiceId {
        &self.invoice_id
    }

    #[must_use]
    pub fn customer_id(&self) -> &BillingCustomerId {
        &self.customer_id
    }

    #[must_use]
    pub fn organization_scope(&self) -> &LedgerScopeId {
        &self.organization_scope
    }

    #[must_use]
    pub const fn status(&self) -> InvoiceStatus {
        self.status
    }

    #[must_use]
    pub const fn currency(&self) -> Currency {
        self.currency
    }

    #[must_use]
    pub const fn amount_due_minor(&self) -> i128 {
        self.amount_due_minor
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BillingLedgerAccounts {
    receivable: AccountId,
    revenue: AccountId,
}
impl BillingLedgerAccounts {
    pub fn new(receivable: AccountId, revenue: AccountId) -> Result<Self, BillingError> {
        if receivable == revenue {
            return Err(BillingError::SamePostingAccount(receivable));
        }
        Ok(Self {
            receivable,
            revenue,
        })
    }

    #[must_use]
    pub fn receivable(&self) -> &AccountId {
        &self.receivable
    }

    #[must_use]
    pub fn revenue(&self) -> &AccountId {
        &self.revenue
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BillingApplyOutcome {
    IgnoredDraft,
    Committed { journal_entry_id: JournalEntryId },
    Replayed { journal_entry_id: JournalEntryId },
}

#[derive(Debug, Clone, Copy, Default)]
pub struct BillingLedgerBridge;
impl BillingLedgerBridge {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    pub fn apply(
        &self,
        event: &InvoiceEvent,
        accounts: &BillingLedgerAccounts,
        ledger: &mut Ledger,
    ) -> Result<BillingApplyOutcome, BillingError> {
        match event.status {
            InvoiceStatus::Draft => return Ok(BillingApplyOutcome::IgnoredDraft),
            InvoiceStatus::Void | InvoiceStatus::Uncollectible => {
                return Err(BillingError::UnsupportedInvoiceStatus(event.status));
            }
            InvoiceStatus::Finalized => {}
        }

        let finalized_at = event
            .finalized_at_unix_ms
            .ok_or(BillingError::MissingFinalizedAt)?;
        if event.amount_due_minor <= 0 {
            return Err(BillingError::InvalidAmountDue(event.amount_due_minor));
        }
        if event.tax_amount_minor != 0 {
            return Err(BillingError::UnsupportedTaxAmount(event.tax_amount_minor));
        }
        if event.amount_due_minor != event.total_minor {
            return Err(BillingError::UnsupportedAmountDueAdjustment {
                amount_due: event.amount_due_minor,
                total: event.total_minor,
            });
        }

        validate_account(
            ledger,
            accounts.receivable(),
            AccountKind::Asset,
            event.organization_scope(),
            event.currency,
        )?;
        validate_account(
            ledger,
            accounts.revenue(),
            AccountKind::Revenue,
            event.organization_scope(),
            event.currency,
        )?;

        let entry_id = journal_entry_id_for_invoice(event.invoice_id())?;
        let correlation = billing_payload_correlation(event);
        let metadata = EntryMetadata::new(
            Some(correlation),
            Some(event.source_event_id.as_str().to_owned()),
        )
        .map_err(BillingError::LedgerBuild)?;
        let postings = vec![
            Posting::new(
                accounts.receivable().clone(),
                event.currency,
                Side::Debit,
                event.amount_due_minor,
            )
            .map_err(BillingError::LedgerBuild)?,
            Posting::new(
                accounts.revenue().clone(),
                event.currency,
                Side::Credit,
                event.amount_due_minor,
            )
            .map_err(BillingError::LedgerBuild)?,
        ];
        let entry = JournalEntry::new(
            entry_id.clone(),
            postings,
            finalized_at,
            event.observed_at_unix_ms,
            metadata,
        )
        .map_err(BillingError::LedgerBuild)?;

        match ledger.commit(entry).map_err(BillingError::LedgerCommit)? {
            CommitOutcome::Committed => Ok(BillingApplyOutcome::Committed {
                journal_entry_id: entry_id,
            }),
            CommitOutcome::Replayed => Ok(BillingApplyOutcome::Replayed {
                journal_entry_id: entry_id,
            }),
        }
    }
}

pub fn journal_entry_id_for_invoice(
    invoice_id: &BillingInvoiceId,
) -> Result<JournalEntryId, BillingError> {
    JournalEntryId::new(format!("billing:invoice:{}:finalized", invoice_id.as_str()))
        .map_err(BillingError::LedgerBuild)
}
fn billing_payload_correlation(event: &InvoiceEvent) -> String {
    format!(
        "billing:{}:{}:{}:{}:{}:{}:{}:{}",
        event.invoice_id.as_str(),
        event.customer_id.as_str(),
        event.organization_scope.as_str(),
        event.currency.code(),
        event.amount_due_minor,
        event.total_minor,
        event.tax_amount_minor,
        event.finalized_at_unix_ms.unwrap_or_default(),
    )
}

fn validate_account(
    ledger: &Ledger,
    account_id: &AccountId,
    expected_kind: AccountKind,
    expected_scope: &LedgerScopeId,
    expected_currency: Currency,
) -> Result<(), BillingError> {
    let account = ledger
        .account(account_id)
        .ok_or_else(|| BillingError::UnknownLedgerAccount(account_id.clone()))?;
    if account.kind() != expected_kind {
        return Err(BillingError::AccountKindMismatch {
            account_id: account_id.clone(),
            expected: expected_kind,
            actual: account.kind(),
        });
    }
    if account.scope_id() != expected_scope {
        return Err(BillingError::AccountScopeMismatch {
            account_id: account_id.clone(),
            expected: expected_scope.clone(),
            actual: account.scope_id().clone(),
        });
    }
    if account.currency() != expected_currency {
        return Err(BillingError::AccountCurrencyMismatch {
            account_id: account_id.clone(),
            expected: expected_currency,
            actual: account.currency(),
        });
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BillingError {
    EmptyIdentifier(&'static str),
    UnsupportedInvoiceStatus(InvoiceStatus),
    MissingFinalizedAt,
    InvalidAmountDue(i128),
    UnsupportedTaxAmount(i128),
    UnsupportedAmountDueAdjustment {
        amount_due: i128,
        total: i128,
    },
    SamePostingAccount(AccountId),
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

impl Display for BillingError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyIdentifier(name) => write!(f, "{name} must not be empty"),
            Self::UnsupportedInvoiceStatus(status) => {
                write!(f, "invoice status is unsupported in P04: {status:?}")
            }
            Self::MissingFinalizedAt => f.write_str("finalized invoice requires finalized_at"),
            Self::InvalidAmountDue(amount) => {
                write!(f, "finalized invoice amount_due must be positive: {amount}")
            }
            Self::UnsupportedTaxAmount(amount) => write!(
                f,
                "P04 does not yet post tax liability; tax_amount must be zero: {amount}"
            ),
            Self::UnsupportedAmountDueAdjustment { amount_due, total } => write!(
                f,
                "P04 requires amount_due to equal total until credit adjustments are modeled: amount_due={amount_due}, total={total}"
            ),
            Self::SamePostingAccount(id) => write!(
                f,
                "receivable and revenue accounts must be distinct: {}",
                id.as_str()
            ),
            Self::UnknownLedgerAccount(id) => {
                write!(
                    f,
                    "billing ledger account is not registered: {}",
                    id.as_str()
                )
            }
            Self::AccountKindMismatch {
                account_id,
                expected,
                actual,
            } => write!(
                f,
                "billing ledger account {} has kind {actual:?}, expected {expected:?}",
                account_id.as_str()
            ),
            Self::AccountScopeMismatch {
                account_id,
                expected,
                actual,
            } => write!(
                f,
                "billing ledger account {} has scope {}, expected {}",
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
                "billing ledger account {} has currency {actual}, expected {expected}",
                account_id.as_str()
            ),
            Self::LedgerBuild(error) => write!(f, "cannot build billing ledger entry: {error}"),
            Self::LedgerCommit(error) => write!(f, "cannot commit billing ledger entry: {error}"),
        }
    }
}

impl Error for BillingError {}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
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

    fn accounts() -> BillingLedgerAccounts {
        BillingLedgerAccounts::new(account_id("accounts-receivable"), account_id("revenue"))
            .unwrap()
    }

    fn ledger() -> Ledger {
        let mut ledger = Ledger::new();
        ledger
            .register_account(Account::new(
                account_id("accounts-receivable"),
                scope("org-1"),
                AccountKind::Asset,
                usd(),
            ))
            .unwrap();
        ledger
            .register_account(Account::new(
                account_id("revenue"),
                scope("org-1"),
                AccountKind::Revenue,
                usd(),
            ))
            .unwrap();
        ledger
    }
    fn event(status: InvoiceStatus, event_id: &str, amount: i128) -> InvoiceEvent {
        InvoiceEvent::new(
            BillingEventId::new(event_id).unwrap(),
            scope("org-1"),
            BillingInvoiceId::new("invoice-1").unwrap(),
            BillingCustomerId::new("customer-1").unwrap(),
            status,
            usd(),
            amount,
            amount,
            0,
            if status == InvoiceStatus::Finalized {
                Some(1_700_000_000_000)
            } else {
                None
            },
            1_700_000_000_100,
        )
    }

    fn balances(ledger: &Ledger) -> (AccountBalance, AccountBalance) {
        (
            ledger.balance(&account_id("accounts-receivable")).unwrap(),
            ledger.balance(&account_id("revenue")).unwrap(),
        )
    }

    #[test]
    fn finalized_invoice_posts_receivable_and_revenue() {
        let mut ledger = ledger();
        let outcome = BillingLedgerBridge::new()
            .apply(
                &event(InvoiceStatus::Finalized, "event-1", 10_000),
                &accounts(),
                &mut ledger,
            )
            .unwrap();

        assert_eq!(
            outcome,
            BillingApplyOutcome::Committed {
                journal_entry_id: journal_entry_id_for_invoice(
                    &BillingInvoiceId::new("invoice-1").unwrap()
                )
                .unwrap(),
            }
        );
        let (receivable, revenue) = balances(&ledger);
        assert_eq!(receivable.debits(), 10_000);
        assert_eq!(receivable.credits(), 0);
        assert_eq!(revenue.debits(), 0);
        assert_eq!(revenue.credits(), 10_000);
        assert_eq!(ledger.entry_count(), 1);
    }

    #[test]
    fn draft_invoice_has_zero_economic_effect() {
        let mut ledger = ledger();
        let outcome = BillingLedgerBridge::new()
            .apply(
                &event(InvoiceStatus::Draft, "event-draft", 10_000),
                &accounts(),
                &mut ledger,
            )
            .unwrap();
        assert_eq!(outcome, BillingApplyOutcome::IgnoredDraft);
        assert_eq!(ledger.entry_count(), 0);
        assert_eq!(
            balances(&ledger),
            (AccountBalance::default(), AccountBalance::default())
        );
    }

    #[test]
    fn void_and_uncollectible_are_explicitly_unsupported() {
        for status in [InvoiceStatus::Void, InvoiceStatus::Uncollectible] {
            let mut ledger = ledger();
            assert_eq!(
                BillingLedgerBridge::new().apply(
                    &event(status, "event-unsupported", 10_000),
                    &accounts(),
                    &mut ledger,
                ),
                Err(BillingError::UnsupportedInvoiceStatus(status))
            );
            assert_eq!(ledger.entry_count(), 0);
        }
    }
    #[test]
    fn finalized_invoice_requires_positive_amount_and_timestamp() {
        for amount in [0, -1] {
            let mut ledger = ledger();
            assert_eq!(
                BillingLedgerBridge::new().apply(
                    &event(InvoiceStatus::Finalized, "event-invalid", amount),
                    &accounts(),
                    &mut ledger,
                ),
                Err(BillingError::InvalidAmountDue(amount))
            );
            assert_eq!(ledger.entry_count(), 0);
        }

        let missing_timestamp = InvoiceEvent::new(
            BillingEventId::new("event-no-time").unwrap(),
            scope("org-1"),
            BillingInvoiceId::new("invoice-no-time").unwrap(),
            BillingCustomerId::new("customer-1").unwrap(),
            InvoiceStatus::Finalized,
            usd(),
            10_000,
            10_000,
            0,
            None,
            1_700_000_000_100,
        );
        let mut ledger = ledger();
        assert_eq!(
            BillingLedgerBridge::new().apply(&missing_timestamp, &accounts(), &mut ledger),
            Err(BillingError::MissingFinalizedAt)
        );
        assert_eq!(ledger.entry_count(), 0);
    }

    #[test]
    fn tax_and_amount_due_adjustments_fail_closed_until_explicitly_modeled() {
        let make = |event_id: &str, amount_due: i128, total: i128, tax: i128| {
            InvoiceEvent::new(
                BillingEventId::new(event_id).unwrap(),
                scope("org-1"),
                BillingInvoiceId::new(format!("invoice-{event_id}")).unwrap(),
                BillingCustomerId::new("customer-1").unwrap(),
                InvoiceStatus::Finalized,
                usd(),
                amount_due,
                total,
                tax,
                Some(1_700_000_000_000),
                1_700_000_000_100,
            )
        };

        let mut ledger = ledger();
        assert_eq!(
            BillingLedgerBridge::new().apply(
                &make("tax", 11_500, 11_500, 1_500),
                &accounts(),
                &mut ledger
            ),
            Err(BillingError::UnsupportedTaxAmount(1_500))
        );
        assert_eq!(ledger.entry_count(), 0);

        assert_eq!(
            BillingLedgerBridge::new().apply(
                &make("credit", 9_000, 10_000, 0),
                &accounts(),
                &mut ledger
            ),
            Err(BillingError::UnsupportedAmountDueAdjustment {
                amount_due: 9_000,
                total: 10_000
            })
        );
        assert_eq!(ledger.entry_count(), 0);
    }

    #[test]
    fn exact_source_event_replay_has_zero_duplicate_effect() {
        let mut ledger = ledger();
        let invoice = event(InvoiceStatus::Finalized, "event-1", 10_000);
        let bridge = BillingLedgerBridge::new();
        let first = bridge.apply(&invoice, &accounts(), &mut ledger).unwrap();
        let second = bridge.apply(&invoice, &accounts(), &mut ledger).unwrap();
        assert!(matches!(first, BillingApplyOutcome::Committed { .. }));
        assert!(matches!(second, BillingApplyOutcome::Replayed { .. }));
        assert_eq!(ledger.entry_count(), 1);
        let (receivable, revenue) = balances(&ledger);
        assert_eq!(receivable.debits(), 10_000);
        assert_eq!(revenue.credits(), 10_000);
    }

    #[test]
    fn conflicting_source_event_reuse_fails_closed() {
        let mut ledger = ledger();
        let bridge = BillingLedgerBridge::new();
        bridge
            .apply(
                &event(InvoiceStatus::Finalized, "event-1", 10_000),
                &accounts(),
                &mut ledger,
            )
            .unwrap();
        let conflict = event(InvoiceStatus::Finalized, "event-1", 11_000);
        assert!(matches!(
            bridge.apply(&conflict, &accounts(), &mut ledger),
            Err(BillingError::LedgerCommit(
                LedgerStateError::IdempotencyConflict(_)
            ))
        ));
        assert_eq!(ledger.entry_count(), 1);
        let (receivable, revenue) = balances(&ledger);
        assert_eq!(receivable.debits(), 10_000);
        assert_eq!(revenue.credits(), 10_000);
    }

    #[test]
    fn same_invoice_with_new_source_event_cannot_create_second_history() {
        let mut ledger = ledger();
        let bridge = BillingLedgerBridge::new();
        bridge
            .apply(
                &event(InvoiceStatus::Finalized, "event-1", 10_000),
                &accounts(),
                &mut ledger,
            )
            .unwrap();
        let duplicate = event(InvoiceStatus::Finalized, "event-2", 10_000);
        assert!(matches!(
            bridge.apply(&duplicate, &accounts(), &mut ledger),
            Err(BillingError::LedgerCommit(
                LedgerStateError::DuplicateEntryId(_)
            ))
        ));
        assert_eq!(ledger.entry_count(), 1);
    }

    #[test]
    fn wrong_account_kind_is_rejected_before_commit() {
        let mut ledger = Ledger::new();
        ledger
            .register_account(Account::new(
                account_id("accounts-receivable"),
                scope("org-1"),
                AccountKind::Liability,
                usd(),
            ))
            .unwrap();
        ledger
            .register_account(Account::new(
                account_id("revenue"),
                scope("org-1"),
                AccountKind::Revenue,
                usd(),
            ))
            .unwrap();
        assert_eq!(
            BillingLedgerBridge::new().apply(
                &event(InvoiceStatus::Finalized, "event-kind", 10_000),
                &accounts(),
                &mut ledger,
            ),
            Err(BillingError::AccountKindMismatch {
                account_id: account_id("accounts-receivable"),
                expected: AccountKind::Asset,
                actual: AccountKind::Liability,
            })
        );
        assert_eq!(ledger.entry_count(), 0);
    }
    #[test]
    fn unknown_account_is_rejected_before_commit() {
        let mut ledger = Ledger::new();
        ledger
            .register_account(Account::new(
                account_id("revenue"),
                scope("org-1"),
                AccountKind::Revenue,
                usd(),
            ))
            .unwrap();
        assert_eq!(
            BillingLedgerBridge::new().apply(
                &event(InvoiceStatus::Finalized, "event-unknown", 10_000),
                &accounts(),
                &mut ledger,
            ),
            Err(BillingError::UnknownLedgerAccount(account_id(
                "accounts-receivable"
            )))
        );
        assert_eq!(ledger.entry_count(), 0);
    }

    #[test]
    fn cross_scope_account_is_rejected_before_commit() {
        let mut ledger = {
            let mut value = Ledger::new();
            value
                .register_account(Account::new(
                    account_id("accounts-receivable"),
                    scope("org-2"),
                    AccountKind::Asset,
                    usd(),
                ))
                .unwrap();
            value
                .register_account(Account::new(
                    account_id("revenue"),
                    scope("org-1"),
                    AccountKind::Revenue,
                    usd(),
                ))
                .unwrap();
            value
        };
        assert!(matches!(
            BillingLedgerBridge::new().apply(
                &event(InvoiceStatus::Finalized, "event-scope", 10_000),
                &accounts(),
                &mut ledger,
            ),
            Err(BillingError::AccountScopeMismatch { .. })
        ));
        assert_eq!(ledger.entry_count(), 0);
    }
    #[test]
    fn currency_mismatch_is_rejected_before_commit() {
        let mut ledger = Ledger::new();
        ledger
            .register_account(Account::new(
                account_id("accounts-receivable"),
                scope("org-1"),
                AccountKind::Asset,
                eur(),
            ))
            .unwrap();
        ledger
            .register_account(Account::new(
                account_id("revenue"),
                scope("org-1"),
                AccountKind::Revenue,
                usd(),
            ))
            .unwrap();
        assert!(matches!(
            BillingLedgerBridge::new().apply(
                &event(InvoiceStatus::Finalized, "event-currency", 10_000),
                &accounts(),
                &mut ledger,
            ),
            Err(BillingError::AccountCurrencyMismatch { .. })
        ));
        assert_eq!(ledger.entry_count(), 0);
    }
    #[test]
    fn billing_identity_is_typed_and_posting_accounts_must_be_distinct() {
        assert_eq!(
            BillingInvoiceId::new("   "),
            Err(BillingError::EmptyIdentifier("billing_invoice_id"))
        );
        assert_eq!(
            BillingCustomerId::new(""),
            Err(BillingError::EmptyIdentifier("billing_customer_id"))
        );
        assert_eq!(
            BillingEventId::new("\t"),
            Err(BillingError::EmptyIdentifier("billing_event_id"))
        );
        assert_eq!(
            BillingLedgerAccounts::new(account_id("same"), account_id("same")),
            Err(BillingError::SamePostingAccount(account_id("same")))
        );
    }

    #[test]
    fn journal_entry_mapping_is_deterministic() {
        let invoice_id = BillingInvoiceId::new("invoice-42").unwrap();
        assert_eq!(
            journal_entry_id_for_invoice(&invoice_id).unwrap().as_str(),
            "billing:invoice:invoice-42:finalized"
        );
    }
}
