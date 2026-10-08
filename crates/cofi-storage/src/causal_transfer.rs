//! Bounded original P23→P27 causal fund transfer reconstruction.
//! All financial transitions occur only on a private historical Ledger.
//! Supplied source/registration/history authenticity remains unproven.

use crate::CodecError;
use crate::causal_allocation::{CausalAllocation, rebuild_causal_allocation};
use crate::fund_movement::{FundMovementFact, decode_fund_movement, verify_fund_movement_journal};
use crate::payout_evidence::decode_payout_evidence;
use cofi_billing::{BillingLedgerAccounts, journal_entry_id_for_invoice};
use cofi_community::CommunityRegistry;
use cofi_fund_allocation_authorization::AuthorizedFundAllocationRegistry;
use cofi_fund_transfer_authorization::{
    AuthorizedFundTransferOutcome, AuthorizedFundTransferRegistry, AuthorizedFundTransferRequest,
};
use cofi_ledger::{AccountId, Ledger};
use cofi_payment_authorization::AuthorizedCaptureRegistry;
use cofi_payout_authorization::AuthorizedPayoutRegistry;
use std::collections::BTreeSet;

#[derive(Debug, Clone)]
pub struct CausalTransfer {
    predecessor: CausalAllocation,
    transfers: AuthorizedFundTransferRegistry,
    ledger: Ledger,
}
impl CausalTransfer {
    #[must_use]
    pub fn captures(&self) -> &AuthorizedCaptureRegistry {
        self.predecessor.captures()
    }
    #[must_use]
    pub fn payouts(&self) -> &AuthorizedPayoutRegistry {
        self.predecessor.payouts()
    }
    #[must_use]
    pub fn allocations(&self) -> &AuthorizedFundAllocationRegistry {
        self.predecessor.allocations()
    }
    #[must_use]
    pub const fn transfers(&self) -> &AuthorizedFundTransferRegistry {
        &self.transfers
    }
    #[must_use]
    pub const fn ledger(&self) -> &Ledger {
        &self.ledger
    }
}
/// Exactly one P23/P24/P25/P26/P27 accepted causal lineage, not generic
/// multi-event tenant hydration. Requires independently authenticated
/// original source+genesis+three historical snapshots to be production proof.
#[allow(clippy::too_many_arguments)]
pub fn rebuild_causal_transfer(
    p25_receipt: &[u8],
    p26_source: &[u8],
    p27_source: &[u8],
    billing_accounts: &BillingLedgerAccounts,
    community: &CommunityRegistry,
    genesis: &Ledger,
    after_p25: &Ledger,
    after_p26: &Ledger,
    after_p27: &Ledger,
) -> Result<CausalTransfer, CodecError> {
    let payout = decode_payout_evidence(p25_receipt)?;
    let alloc_source = decode_fund_movement(p26_source)?;
    let FundMovementFact::Allocation {
        event: allocation_event,
        ..
    } = alloc_source
    else {
        return Err(CodecError::Replay("P26 expected allocation source".into()));
    };
    let transfer_source = decode_fund_movement(p27_source)?;
    let FundMovementFact::Transfer(event) = transfer_source else {
        return Err(CodecError::Replay("P27 expected transfer source".into()));
    };
    let journal = verify_fund_movement_journal(
        &FundMovementFact::Transfer(event.clone()),
        community,
        after_p27,
    )?;
    let predecessor = rebuild_causal_allocation(
        p25_receipt,
        p26_source,
        billing_accounts,
        community,
        genesis,
        after_p25,
        after_p26,
    )?;
    let original_allocation = predecessor
        .allocations()
        .authorization_for_event(allocation_event.source_event_id())
        .ok_or_else(|| CodecError::Replay("original P26 accepted allocation absent".into()))?
        .clone();
    let request = AuthorizedFundTransferRequest::new(
        event.source_event_id().clone(),
        event.transfer_id().clone(),
        event.destination_fund_id().clone(),
        original_allocation,
        event.effective_at_unix_ms(),
        event.observed_at_unix_ms(),
    );
    let mut ledger = predecessor.ledger().clone();
    let mut transfers = AuthorizedFundTransferRegistry::new();
    let authorized = match transfers
        .apply(community, request, &mut ledger)
        .map_err(|e| CodecError::Replay(e.to_string()))?
    {
        AuthorizedFundTransferOutcome::Created { authorization } => authorization,
        AuthorizedFundTransferOutcome::Replayed { .. } => {
            return Err(CodecError::Replay(
                "P27 must be newly committed on historical Ledger".into(),
            ));
        }
    };
    if authorized.transfer_event() != &event
        || authorized.journal_entry_id() != &journal
        || transfers.allocation_transfer_count() != 1
    {
        return Err(CodecError::Replay(
            "P27 accepted original transfer differs from typed source".into(),
        ));
    }
    if ledger.entry_count() != 5 || after_p27.entry_count() != 5 {
        return Err(CodecError::Replay(
            "P27 requires exactly five historical journals".into(),
        ));
    }
    let invoice_id = journal_entry_id_for_invoice(
        payout
            .capture()
            .request()
            .authorized_finalization()
            .finalized()
            .draft()
            .invoice_id(),
    )
    .map_err(|e| CodecError::Replay(e.to_string()))?;
    let allocation_id = authorized
        .authorized_allocation()
        .journal_entry_id()
        .clone();
    let ids = [
        invoice_id,
        payout.capture().journal_entry_id().clone(),
        payout.journal_entry_id().clone(),
        allocation_id,
        journal,
    ];
    for id in ids {
        match (ledger.entry(&id), after_p27.entry(&id)) {
            (Some(actual), Some(original)) if actual == original => {}
            _ => {
                return Err(CodecError::Replay(
                    "P27 original journal differs from supplied reference".into(),
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
    accounts.insert(
        authorized
            .authorized_allocation()
            .source_cash_account_id()
            .clone(),
    );
    for fund_id in [event.source_fund_id(), event.destination_fund_id()] {
        let fund = community
            .fund(fund_id)
            .ok_or_else(|| CodecError::Replay("P27 original fund registration missing".into()))?;
        accounts.insert(fund.ledger_account_id().clone());
    }
    for id in accounts {
        if genesis.account(&id) != after_p27.account(&id)
            || ledger.account(&id) != after_p27.account(&id)
            || ledger.balance(&id) != after_p27.balance(&id)
        {
            return Err(CodecError::Replay(
                "P27 original account or running balance differs".into(),
            ));
        }
    }
    Ok(CausalTransfer {
        predecessor,
        transfers,
        ledger,
    })
}
