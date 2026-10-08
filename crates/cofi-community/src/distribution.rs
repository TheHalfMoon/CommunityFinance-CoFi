use std::error::Error;
use std::fmt::{Display, Formatter};

use cofi_ledger::{
    AccountId, AccountKind, CommitOutcome, Currency, EntryMetadata, JournalEntry, JournalEntryId,
    Ledger, LedgerError, LedgerScopeId, LedgerStateError, Posting, Side,
};

use crate::{CommunityId, CommunityRegistry, FundId, OrganizationId};

macro_rules! distribution_id {
    ($name:ident, $label:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, RevenueDistributionError> {
                let value = value.into();
                if value.trim().is_empty() {
                    return Err(RevenueDistributionError::EmptyIdentifier($label));
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
distribution_id!(RevenueSplitRuleId, "revenue_split_rule_id");
distribution_id!(RevenueDistributionEventId, "revenue_distribution_event_id");
distribution_id!(RevenueDistributionId, "revenue_distribution_id");

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BasisPoints(u16);

impl BasisPoints {
    pub fn new(value: u16) -> Result<Self, RevenueDistributionError> {
        if !(1..=10_000).contains(&value) {
            return Err(RevenueDistributionError::InvalidBasisPoints(value));
        }
        Ok(Self(value))
    }

    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevenueSplitLeg {
    destination_fund_id: FundId,
    basis_points: BasisPoints,
}

impl RevenueSplitLeg {
    #[must_use]
    pub const fn new(destination_fund_id: FundId, basis_points: BasisPoints) -> Self {
        Self {
            destination_fund_id,
            basis_points,
        }
    }

    #[must_use]
    pub const fn destination_fund_id(&self) -> &FundId {
        &self.destination_fund_id
    }

    #[must_use]
    pub const fn basis_points(&self) -> BasisPoints {
        self.basis_points
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevenueSplitRule {
    id: RevenueSplitRuleId,
    version: u32,
    legs: Vec<RevenueSplitLeg>,
}

impl RevenueSplitRule {
    pub fn new(
        id: RevenueSplitRuleId,
        version: u32,
        mut legs: Vec<RevenueSplitLeg>,
    ) -> Result<Self, RevenueDistributionError> {
        if version == 0 {
            return Err(RevenueDistributionError::InvalidRuleVersion);
        }
        if legs.len() < 2 {
            return Err(RevenueDistributionError::TooFewRuleLegs);
        }
        legs.sort_by(|a, b| {
            a.destination_fund_id
                .as_str()
                .cmp(b.destination_fund_id.as_str())
        });
        for pair in legs.windows(2) {
            if pair[0].destination_fund_id == pair[1].destination_fund_id {
                return Err(RevenueDistributionError::DuplicateDestinationFund(
                    pair[0].destination_fund_id.clone(),
                ));
            }
        }
        let total_bps = legs.iter().try_fold(0_u32, |sum, leg| {
            sum.checked_add(u32::from(leg.basis_points.get()))
                .ok_or(RevenueDistributionError::ArithmeticOverflow)
        })?;
        if total_bps != 10_000 {
            return Err(RevenueDistributionError::InvalidBasisPointTotal(total_bps));
        }
        Ok(Self { id, version, legs })
    }

    #[must_use]
    pub const fn id(&self) -> &RevenueSplitRuleId {
        &self.id
    }

    #[must_use]
    pub const fn version(&self) -> u32 {
        self.version
    }

    #[must_use]
    pub fn legs(&self) -> &[RevenueSplitLeg] {
        &self.legs
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevenueDistributionEvent {
    source_event_id: RevenueDistributionEventId,
    organization_scope: LedgerScopeId,
    source_fund_id: FundId,
    distribution_id: RevenueDistributionId,
    rule_id: RevenueSplitRuleId,
    rule_version: u32,
    currency: Currency,
    amount_minor: i128,
    effective_at_unix_ms: i64,
    observed_at_unix_ms: i64,
}

impl RevenueDistributionEvent {
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        source_event_id: RevenueDistributionEventId,
        organization_scope: LedgerScopeId,
        source_fund_id: FundId,
        distribution_id: RevenueDistributionId,
        rule_id: RevenueSplitRuleId,
        rule_version: u32,
        currency: Currency,
        amount_minor: i128,
        effective_at_unix_ms: i64,
        observed_at_unix_ms: i64,
    ) -> Self {
        Self {
            source_event_id,
            organization_scope,
            source_fund_id,
            distribution_id,
            rule_id,
            rule_version,
            currency,
            amount_minor,
            effective_at_unix_ms,
            observed_at_unix_ms,
        }
    }

    /// Original accepted source identity and immutable rule evidence.
    #[must_use]
    pub const fn source_event_id(&self) -> &RevenueDistributionEventId {
        &self.source_event_id
    }
    #[must_use]
    pub const fn rule_id(&self) -> &RevenueSplitRuleId {
        &self.rule_id
    }
    #[must_use]
    pub const fn rule_version(&self) -> u32 {
        self.rule_version
    }
    #[must_use]
    pub const fn effective_at_unix_ms(&self) -> i64 {
        self.effective_at_unix_ms
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
    pub fn source_fund_id(&self) -> &FundId {
        &self.source_fund_id
    }
    #[must_use]
    pub fn distribution_id(&self) -> &RevenueDistributionId {
        &self.distribution_id
    }
    #[must_use]
    pub const fn currency(&self) -> Currency {
        self.currency
    }
    #[must_use]
    pub const fn amount_minor(&self) -> i128 {
        self.amount_minor
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RevenueDistributionOutcome {
    Committed { journal_entry_id: JournalEntryId },
    Replayed { journal_entry_id: JournalEntryId },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ComputedLeg {
    fund_id: FundId,
    amount_minor: i128,
    remainder: u128,
}

fn compute_allocations(
    amount_minor: i128,
    rule: &RevenueSplitRule,
) -> Result<Vec<ComputedLeg>, RevenueDistributionError> {
    if amount_minor <= 0 {
        return Err(RevenueDistributionError::InvalidAmount(amount_minor));
    }
    let total = u128::try_from(amount_minor)
        .map_err(|_| RevenueDistributionError::InvalidAmount(amount_minor))?;
    let mut legs = Vec::with_capacity(rule.legs().len());
    let mut allocated = 0_u128;
    for leg in rule.legs() {
        let product = total
            .checked_mul(u128::from(leg.basis_points().get()))
            .ok_or(RevenueDistributionError::ArithmeticOverflow)?;
        let base = product / 10_000;
        allocated = allocated
            .checked_add(base)
            .ok_or(RevenueDistributionError::ArithmeticOverflow)?;
        let base_i128 =
            i128::try_from(base).map_err(|_| RevenueDistributionError::ArithmeticOverflow)?;
        legs.push(ComputedLeg {
            fund_id: leg.destination_fund_id().clone(),
            amount_minor: base_i128,
            remainder: product % 10_000,
        });
    }
    let leftover = total
        .checked_sub(allocated)
        .ok_or(RevenueDistributionError::ArithmeticOverflow)?;
    let leftover =
        usize::try_from(leftover).map_err(|_| RevenueDistributionError::ArithmeticOverflow)?;
    if leftover > legs.len() {
        return Err(RevenueDistributionError::ArithmeticOverflow);
    }

    let mut order: Vec<usize> = (0..legs.len()).collect();
    order.sort_by(|&a, &b| {
        legs[b]
            .remainder
            .cmp(&legs[a].remainder)
            .then_with(|| legs[a].fund_id.as_str().cmp(legs[b].fund_id.as_str()))
    });
    for index in order.into_iter().take(leftover) {
        legs[index].amount_minor = legs[index]
            .amount_minor
            .checked_add(1)
            .ok_or(RevenueDistributionError::ArithmeticOverflow)?;
    }
    if let Some(leg) = legs.iter().find(|leg| leg.amount_minor == 0) {
        return Err(RevenueDistributionError::ZeroDestinationAllocation(
            leg.fund_id.clone(),
        ));
    }
    let exact_total = legs.iter().try_fold(0_i128, |sum, leg| {
        sum.checked_add(leg.amount_minor)
            .ok_or(RevenueDistributionError::ArithmeticOverflow)
    })?;
    if exact_total != amount_minor {
        return Err(RevenueDistributionError::AllocationTotalMismatch {
            expected: amount_minor,
            actual: exact_total,
        });
    }
    Ok(legs)
}

#[derive(Debug, Clone, Copy, Default)]
pub struct RevenueDistributionBridge;

impl RevenueDistributionBridge {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    pub fn apply(
        &self,
        registry: &CommunityRegistry,
        event: &RevenueDistributionEvent,
        rule: &RevenueSplitRule,
        ledger: &mut Ledger,
    ) -> Result<RevenueDistributionOutcome, RevenueDistributionError> {
        if event.rule_id != *rule.id() || event.rule_version != rule.version() {
            return Err(RevenueDistributionError::RuleIdentityMismatch);
        }
        let computed = compute_allocations(event.amount_minor(), rule)?;
        let source_fund = registry
            .fund(event.source_fund_id())
            .ok_or_else(|| RevenueDistributionError::UnknownFund(event.source_fund_id().clone()))?;
        let source_community = registry
            .community(source_fund.community_id())
            .ok_or_else(|| {
                RevenueDistributionError::MissingFundCommunity(source_fund.community_id().clone())
            })?;
        validate_fund(
            source_fund.id(),
            source_community.organization_id(),
            source_fund.currency(),
            event.organization_scope(),
            event.currency(),
        )?;
        validate_asset_account(
            ledger,
            source_fund.ledger_account_id(),
            event.organization_scope(),
            event.currency(),
        )?;

        let mut destination_accounts = Vec::with_capacity(computed.len());
        for leg in &computed {
            if &leg.fund_id == event.source_fund_id() {
                return Err(RevenueDistributionError::SourceIsDestination(
                    leg.fund_id.clone(),
                ));
            }
            let fund = registry
                .fund(&leg.fund_id)
                .ok_or_else(|| RevenueDistributionError::UnknownFund(leg.fund_id.clone()))?;
            let community = registry.community(fund.community_id()).ok_or_else(|| {
                RevenueDistributionError::MissingFundCommunity(fund.community_id().clone())
            })?;
            validate_fund(
                fund.id(),
                community.organization_id(),
                fund.currency(),
                event.organization_scope(),
                event.currency(),
            )?;
            validate_asset_account(
                ledger,
                fund.ledger_account_id(),
                event.organization_scope(),
                event.currency(),
            )?;
            if fund.ledger_account_id() == source_fund.ledger_account_id()
                || destination_accounts
                    .iter()
                    .any(|id| id == fund.ledger_account_id())
            {
                return Err(RevenueDistributionError::DuplicatePostingAccount(
                    fund.ledger_account_id().clone(),
                ));
            }
            destination_accounts.push(fund.ledger_account_id().clone());
        }

        let entry_id = journal_entry_id_for_revenue_distribution(event.distribution_id())?;
        let metadata = EntryMetadata::new(
            Some(distribution_payload_correlation(event, rule, &computed)),
            Some(event.source_event_id.as_str().to_owned()),
        )
        .and_then(|metadata| {
            metadata.with_business_key(Some(format!(
                "community:revenue-distribution:{}",
                event.distribution_id.as_str()
            )))
        })
        .map_err(RevenueDistributionError::LedgerBuild)?;
        let mut postings = Vec::with_capacity(computed.len() + 1);
        for (leg, account_id) in computed.iter().zip(destination_accounts.iter()) {
            postings.push(
                Posting::new(
                    account_id.clone(),
                    event.currency(),
                    Side::Debit,
                    leg.amount_minor,
                )
                .map_err(RevenueDistributionError::LedgerBuild)?,
            );
        }
        postings.push(
            Posting::new(
                source_fund.ledger_account_id().clone(),
                event.currency(),
                Side::Credit,
                event.amount_minor(),
            )
            .map_err(RevenueDistributionError::LedgerBuild)?,
        );

        let entry = JournalEntry::new(
            entry_id.clone(),
            postings,
            event.effective_at_unix_ms,
            event.observed_at_unix_ms,
            metadata,
        )
        .map_err(RevenueDistributionError::LedgerBuild)?;

        if ledger.entry(&entry_id) == Some(&entry) {
            return Ok(RevenueDistributionOutcome::Replayed {
                journal_entry_id: entry_id,
            });
        }

        let balance = ledger
            .balance(source_fund.ledger_account_id())
            .ok_or_else(|| {
                RevenueDistributionError::UnknownLedgerAccount(
                    source_fund.ledger_account_id().clone(),
                )
            })?;
        let available = balance.debits().checked_sub(balance.credits()).ok_or(
            RevenueDistributionError::SourceFundNegativeBalance {
                debits: balance.debits(),
                credits: balance.credits(),
            },
        )?;
        let requested = u128::try_from(event.amount_minor())
            .map_err(|_| RevenueDistributionError::InvalidAmount(event.amount_minor()))?;
        if available < requested {
            return Err(RevenueDistributionError::InsufficientSourceFund {
                available,
                requested,
            });
        }

        match ledger
            .commit(entry)
            .map_err(RevenueDistributionError::LedgerCommit)?
        {
            CommitOutcome::Committed => Ok(RevenueDistributionOutcome::Committed {
                journal_entry_id: entry_id,
            }),
            CommitOutcome::Replayed => Ok(RevenueDistributionOutcome::Replayed {
                journal_entry_id: entry_id,
            }),
        }
    }
}

pub fn journal_entry_id_for_revenue_distribution(
    distribution_id: &RevenueDistributionId,
) -> Result<JournalEntryId, RevenueDistributionError> {
    JournalEntryId::new(format!(
        "community:revenue-distribution:{}",
        distribution_id.as_str()
    ))
    .map_err(RevenueDistributionError::LedgerBuild)
}

fn distribution_payload_correlation(
    event: &RevenueDistributionEvent,
    rule: &RevenueSplitRule,
    computed: &[ComputedLeg],
) -> String {
    let legs = rule
        .legs()
        .iter()
        .zip(computed.iter())
        .map(|(rule_leg, computed_leg)| {
            format!(
                "{}:{}:{}",
                rule_leg.destination_fund_id().as_str(),
                rule_leg.basis_points().get(),
                computed_leg.amount_minor,
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "revenue-distribution:{}:{}:{}:{}:{}:{}:{}:{}:{}",
        event.source_fund_id.as_str(),
        event.distribution_id.as_str(),
        rule.id().as_str(),
        rule.version(),
        event.organization_scope.as_str(),
        event.currency.code(),
        event.amount_minor,
        event.effective_at_unix_ms,
        legs,
    )
}

fn validate_fund(
    fund_id: &FundId,
    organization_id: &OrganizationId,
    fund_currency: Currency,
    expected_scope: &LedgerScopeId,
    expected_currency: Currency,
) -> Result<(), RevenueDistributionError> {
    if organization_id.as_str() != expected_scope.as_str() {
        return Err(RevenueDistributionError::OrganizationScopeMismatch {
            fund_id: fund_id.clone(),
            fund_organization_id: organization_id.clone(),
            event_scope: expected_scope.clone(),
        });
    }
    if fund_currency != expected_currency {
        return Err(RevenueDistributionError::FundCurrencyMismatch {
            fund_id: fund_id.clone(),
            expected: fund_currency,
            actual: expected_currency,
        });
    }
    Ok(())
}

fn validate_asset_account(
    ledger: &Ledger,
    account_id: &AccountId,
    expected_scope: &LedgerScopeId,
    expected_currency: Currency,
) -> Result<(), RevenueDistributionError> {
    let account = ledger
        .account(account_id)
        .ok_or_else(|| RevenueDistributionError::UnknownLedgerAccount(account_id.clone()))?;
    if account.kind() != AccountKind::Asset {
        return Err(RevenueDistributionError::AccountKindMismatch {
            account_id: account_id.clone(),
            actual: account.kind(),
        });
    }
    if account.scope_id() != expected_scope {
        return Err(RevenueDistributionError::AccountScopeMismatch {
            account_id: account_id.clone(),
            expected: expected_scope.clone(),
            actual: account.scope_id().clone(),
        });
    }
    if account.currency() != expected_currency {
        return Err(RevenueDistributionError::AccountCurrencyMismatch {
            account_id: account_id.clone(),
            expected: expected_currency,
            actual: account.currency(),
        });
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RevenueDistributionError {
    EmptyIdentifier(&'static str),
    InvalidRuleVersion,
    TooFewRuleLegs,
    InvalidBasisPoints(u16),
    InvalidBasisPointTotal(u32),
    DuplicateDestinationFund(FundId),
    InvalidAmount(i128),
    ArithmeticOverflow,
    ZeroDestinationAllocation(FundId),
    AllocationTotalMismatch {
        expected: i128,
        actual: i128,
    },
    RuleIdentityMismatch,
    UnknownFund(FundId),
    MissingFundCommunity(CommunityId),
    SourceIsDestination(FundId),
    OrganizationScopeMismatch {
        fund_id: FundId,
        fund_organization_id: OrganizationId,
        event_scope: LedgerScopeId,
    },
    FundCurrencyMismatch {
        fund_id: FundId,
        expected: Currency,
        actual: Currency,
    },
    DuplicatePostingAccount(AccountId),
    UnknownLedgerAccount(AccountId),
    AccountKindMismatch {
        account_id: AccountId,
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
    SourceFundNegativeBalance {
        debits: u128,
        credits: u128,
    },
    InsufficientSourceFund {
        available: u128,
        requested: u128,
    },
    LedgerBuild(LedgerError),
    LedgerCommit(LedgerStateError),
}

impl Display for RevenueDistributionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyIdentifier(name) => write!(f, "{name} must not be empty"),
            Self::InvalidRuleVersion => write!(f, "revenue split rule version must be non-zero"),
            Self::TooFewRuleLegs => write!(f, "revenue split rule requires at least two legs"),
            Self::InvalidBasisPoints(value) => write!(f, "invalid basis points: {value}"),
            Self::InvalidBasisPointTotal(value) => {
                write!(
                    f,
                    "revenue split basis points must total 10000; got {value}"
                )
            }
            Self::DuplicateDestinationFund(id) => {
                write!(f, "duplicate destination fund: {}", id.as_str())
            }
            Self::InvalidAmount(value) => {
                write!(f, "revenue distribution amount must be positive: {value}")
            }
            Self::ArithmeticOverflow => write!(f, "revenue distribution arithmetic overflow"),
            Self::ZeroDestinationAllocation(id) => write!(
                f,
                "distribution would create a zero-value posting for fund {}",
                id.as_str()
            ),
            Self::AllocationTotalMismatch { expected, actual } => write!(
                f,
                "computed distribution total {actual} does not equal expected {expected}"
            ),
            Self::RuleIdentityMismatch => write!(f, "event rule identity/version mismatch"),
            Self::UnknownFund(id) => write!(f, "unknown fund: {}", id.as_str()),
            Self::MissingFundCommunity(id) => {
                write!(f, "fund references a missing community: {}", id.as_str())
            }
            Self::SourceIsDestination(id) => {
                write!(f, "source fund cannot be a destination: {}", id.as_str())
            }
            Self::OrganizationScopeMismatch {
                fund_id,
                fund_organization_id,
                event_scope,
            } => write!(
                f,
                "fund {} belongs to organization {}; distribution scope is {}",
                fund_id.as_str(),
                fund_organization_id.as_str(),
                event_scope.as_str()
            ),
            Self::FundCurrencyMismatch {
                fund_id,
                expected,
                actual,
            } => write!(
                f,
                "fund {} uses currency {expected}; distribution uses {actual}",
                fund_id.as_str()
            ),
            Self::DuplicatePostingAccount(id) => write!(
                f,
                "distribution backing accounts must be distinct: {}",
                id.as_str()
            ),
            Self::UnknownLedgerAccount(id) => {
                write!(f, "unknown distribution ledger account: {}", id.as_str())
            }
            Self::AccountKindMismatch { account_id, actual } => write!(
                f,
                "distribution ledger account {} has kind {actual:?}; expected Asset",
                account_id.as_str()
            ),
            Self::AccountScopeMismatch {
                account_id,
                expected,
                actual,
            } => write!(
                f,
                "distribution ledger account {} has scope {}; expected {}",
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
                "distribution ledger account {} has currency {actual}; expected {expected}",
                account_id.as_str()
            ),
            Self::SourceFundNegativeBalance { debits, credits } => write!(
                f,
                "source fund has negative debit-normal balance: debits={debits}, credits={credits}"
            ),
            Self::InsufficientSourceFund {
                available,
                requested,
            } => write!(
                f,
                "source fund balance is insufficient: available={available}, requested={requested}"
            ),
            Self::LedgerBuild(error) => write!(f, "failed to build revenue distribution: {error}"),
            Self::LedgerCommit(error) => {
                write!(f, "failed to commit revenue distribution: {error}")
            }
        }
    }
}

impl Error for RevenueDistributionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::LedgerBuild(error) => Some(error),
            Self::LedgerCommit(error) => Some(error),
            _ => None,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    fn fund(value: &str) -> FundId {
        FundId::new(value).unwrap()
    }
    fn rule_id(value: &str) -> RevenueSplitRuleId {
        RevenueSplitRuleId::new(value).unwrap()
    }

    fn leg(id: &str, bps: u16) -> RevenueSplitLeg {
        RevenueSplitLeg::new(fund(id), BasisPoints::new(bps).unwrap())
    }

    #[test]
    fn rule_is_canonicalized_and_requires_exact_total() {
        let rule =
            RevenueSplitRule::new(rule_id("rule-1"), 1, vec![leg("z", 4_000), leg("a", 6_000)])
                .unwrap();
        assert_eq!(rule.legs()[0].destination_fund_id().as_str(), "a");
        assert_eq!(rule.legs()[1].destination_fund_id().as_str(), "z");
        assert_eq!(
            RevenueSplitRule::new(rule_id("bad"), 1, vec![leg("a", 5_000), leg("b", 4_999)]),
            Err(RevenueDistributionError::InvalidBasisPointTotal(9_999))
        );
    }

    #[test]
    fn rule_rejects_duplicate_destination_and_zero_version() {
        assert_eq!(
            RevenueSplitRule::new(rule_id("rule"), 1, vec![leg("a", 5_000), leg("a", 5_000)]),
            Err(RevenueDistributionError::DuplicateDestinationFund(fund(
                "a"
            )))
        );
        assert_eq!(
            RevenueSplitRule::new(rule_id("rule"), 0, vec![leg("a", 5_000), leg("b", 5_000)]),
            Err(RevenueDistributionError::InvalidRuleVersion)
        );
    }

    #[test]
    fn largest_remainder_is_deterministic() {
        let rule = RevenueSplitRule::new(
            rule_id("rule"),
            1,
            vec![leg("c", 3_334), leg("b", 3_333), leg("a", 3_333)],
        )
        .unwrap();
        let amounts = compute_allocations(100, &rule).unwrap();
        assert_eq!(amounts[0].amount_minor, 33);
        assert_eq!(amounts[1].amount_minor, 33);
        assert_eq!(amounts[2].amount_minor, 34);
    }

    #[test]
    fn equal_remainder_tie_uses_canonical_fund_id() {
        let rule =
            RevenueSplitRule::new(rule_id("rule"), 1, vec![leg("z", 5_000), leg("a", 5_000)])
                .unwrap();
        let amounts = compute_allocations(3, &rule).unwrap();
        assert_eq!(amounts[0].fund_id.as_str(), "a");
        assert_eq!(amounts[0].amount_minor, 2);
        assert_eq!(amounts[1].amount_minor, 1);
    }

    #[test]
    fn zero_destination_and_overflow_fail_closed() {
        let rule =
            RevenueSplitRule::new(rule_id("rule"), 1, vec![leg("a", 5_000), leg("b", 5_000)])
                .unwrap();
        assert!(matches!(
            compute_allocations(1, &rule),
            Err(RevenueDistributionError::ZeroDestinationAllocation(_))
        ));
        assert_eq!(
            compute_allocations(i128::MAX, &rule),
            Err(RevenueDistributionError::ArithmeticOverflow)
        );
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod rule_validation_tests {
    use super::*;

    #[test]
    fn basis_points_and_minimum_leg_count_are_validated() {
        assert_eq!(
            BasisPoints::new(0),
            Err(RevenueDistributionError::InvalidBasisPoints(0))
        );
        assert_eq!(
            BasisPoints::new(10_001),
            Err(RevenueDistributionError::InvalidBasisPoints(10_001))
        );
        let only = RevenueSplitLeg::new(
            FundId::new("only-fund").expect("valid fund"),
            BasisPoints::new(10_000).expect("valid bps"),
        );
        assert_eq!(
            RevenueSplitRule::new(
                RevenueSplitRuleId::new("rule-single").expect("valid rule"),
                1,
                vec![only],
            ),
            Err(RevenueDistributionError::TooFewRuleLegs)
        );
    }
}
