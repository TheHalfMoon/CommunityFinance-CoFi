//! One-flow causal payout → authorized allocation replay, using only
//! original domain authorization logic on an isolated historical Ledger.
//! External trust/completeness of supplied references is NOT established.

use crate::CodecError;
use crate::causal_capture_payout::{CausalCapturePayout, rebuild_causal_capture_payout};
use crate::fund_movement::{FundMovementFact, decode_fund_movement, verify_fund_movement_journal};
use crate::payout_evidence::decode_payout_evidence;
use cofi_billing::BillingLedgerAccounts;
use cofi_community::CommunityRegistry;
use cofi_fund_allocation_authorization::{
    AuthorizedFundAllocationOutcome, AuthorizedFundAllocationRegistry,
    AuthorizedFundAllocationRequest,
};
use cofi_ledger::{AccountId, Ledger};
use cofi_payment_authorization::AuthorizedCaptureRegistry;
use cofi_payout_authorization::AuthorizedPayoutRegistry;
use std::collections::BTreeSet;

#[derive(Debug, Clone)]
pub struct CausalAllocation {
    predecessor: CausalCapturePayout,
    allocations: AuthorizedFundAllocationRegistry,
    ledger: Ledger,
}
impl CausalAllocation {
    #[must_use]
    pub fn captures(&self) -> &AuthorizedCaptureRegistry {
        self.predecessor.captures()
    }
    #[must_use]
    pub fn payouts(&self) -> &AuthorizedPayoutRegistry {
        self.predecessor.payouts()
    }
    #[must_use]
    pub const fn allocations(&self) -> &AuthorizedFundAllocationRegistry {
        &self.allocations
    }
    #[must_use]
    pub const fn ledger(&self) -> &Ledger {
        &self.ledger
    }
}

/// Reconstruct ONE original P23/P24/P25/P26 causal flow with externally
/// provided account-only genesis, accepted after-P25 and after-P26 snapshots.
/// The intermediate after-P25 snapshot is mandatory: simply replaying P26
/// against the already fully-hydrated final Ledger would bypass its
/// mandatory first-time Committed authorization/index transition.
#[allow(clippy::too_many_arguments)]
pub fn rebuild_causal_allocation(
    p25_receipt: &[u8],
    p26_source: &[u8],
    billing_accounts: &BillingLedgerAccounts,
    community: &CommunityRegistry,
    genesis: &Ledger,
    after_p25: &Ledger,
    after_p26: &Ledger,
) -> Result<CausalAllocation, CodecError> {
    let payout = decode_payout_evidence(p25_receipt)?;
    let allocation = decode_fund_movement(p26_source)?;
    let FundMovementFact::Allocation { event, source_cash } = allocation else {
        return Err(CodecError::Replay(
            "P26 expected original allocation source, not transfer".into(),
        ));
    };
    let accepted_journal = verify_fund_movement_journal(
        &FundMovementFact::Allocation {
            event: event.clone(),
            source_cash: source_cash.clone(),
        },
        community,
        after_p26,
    )?;
    let predecessor =
        rebuild_causal_capture_payout(p25_receipt, billing_accounts, genesis, after_p25)?;
    let original_payout = predecessor
        .payouts()
        .authorization_for_event(payout.event().source_event_id())
        .ok_or_else(|| CodecError::Replay("original P25 accepted payout absent".into()))?
        .clone();
    let request = AuthorizedFundAllocationRequest::new(
        event.source_event_id().clone(),
        event.allocation_id().clone(),
        event.fund_id().clone(),
        original_payout,
        event.effective_at_unix_ms(),
        event.observed_at_unix_ms(),
    );
    let mut ledger = predecessor.ledger().clone();
    let mut allocations = AuthorizedFundAllocationRegistry::new();
    let accepted = match allocations
        .apply(community, request, &mut ledger)
        .map_err(|e| CodecError::Replay(e.to_string()))?
    {
        AuthorizedFundAllocationOutcome::Created { authorization } => authorization,
        AuthorizedFundAllocationOutcome::Replayed { .. } => {
            return Err(CodecError::Replay(
                "P26 unexpectedly replayed, not authorized anew".into(),
            ));
        }
    };
    if accepted.allocation_event() != &event
        || accepted.source_cash_account_id() != &source_cash
        || accepted.journal_entry_id() != &accepted_journal
        || allocations.payout_allocation_count() != 1
    {
        return Err(CodecError::Replay(
            "P26 original accepted authority differs from recorded source".into(),
        ));
    }

    if ledger.entry_count() != 4 || after_p26.entry_count() != 4 {
        return Err(CodecError::Replay(
            "expected exactly P23/P24/P25/P26 accepted journals".into(),
        ));
    }
    let invoice_id = cofi_billing::journal_entry_id_for_invoice(
        payout
            .capture()
            .request()
            .authorized_finalization()
            .finalized()
            .draft()
            .invoice_id(),
    )
    .map_err(|e| CodecError::Replay(e.to_string()))?;
    let ids = [
        invoice_id,
        payout.capture().journal_entry_id().clone(),
        payout.journal_entry_id().clone(),
        accepted_journal,
    ];
    for id in ids {
        match (ledger.entry(&id), after_p26.entry(&id)) {
            (Some(actual), Some(original)) if actual == original => {}
            _ => {
                return Err(CodecError::Replay(
                    "P26 historical journal differs from reference".into(),
                ));
            }
        }
    }
    let mut accounts: BTreeSet<AccountId> = BTreeSet::new();
    accounts.insert(billing_accounts.receivable().clone());
    accounts.insert(billing_accounts.revenue().clone());
    accounts.insert(payout.capture().accounts().receivable().clone());
    accounts.insert(payout.capture().accounts().processor_clearing().clone());
    accounts.insert(payout.accounts().processor_clearing().clone());
    accounts.insert(payout.accounts().bank_cash().clone());
    if let Some(fee) = payout.accounts().processor_fee_expense() {
        accounts.insert(fee.clone());
    }
    accounts.insert(source_cash);
    let fund = community
        .fund(event.fund_id())
        .ok_or_else(|| CodecError::Replay("original community fund missing".into()))?;
    accounts.insert(fund.ledger_account_id().clone());
    for id in accounts {
        if ledger.account(&id) != after_p26.account(&id)
            || genesis.account(&id) != after_p26.account(&id)
            || ledger.balance(&id) != after_p26.balance(&id)
        {
            return Err(CodecError::Replay(
                "P26 original account or balance differs from reference".into(),
            ));
        }
    }
    Ok(CausalAllocation {
        predecessor,
        allocations,
        ledger,
    })
}
