//! Bounded staged, shared multi-tenant P23 -> P27 authorized replay.
//! The ORIGINAL domain registries consume payout/allocation authority on a
//! PRIVATE Ledger. Caller references and stage order are not authenticated.
//! Only complete flows ordered as all P23-P25, then all P26, then all P27.

use std::collections::BTreeSet;

use cofi_billing::{BillingLedgerAccounts, journal_entry_id_for_invoice};
use cofi_community::CommunityRegistry;
use cofi_fund_allocation_authorization::{
    AuthorizedFundAllocationOutcome, AuthorizedFundAllocationRegistry,
    AuthorizedFundAllocationRequest,
};
use cofi_fund_transfer_authorization::{
    AuthorizedFundTransferOutcome, AuthorizedFundTransferRegistry, AuthorizedFundTransferRequest,
};
use cofi_ledger::{AccountId, JournalEntryId, Ledger};

use crate::CodecError;
use crate::causal_capture_payout_stream::{
    CausalPaymentFlow, CausalPaymentStream, rebuild_causal_capture_payout_stream,
};
use crate::fund_movement::{FundMovementFact, decode_fund_movement, verify_fund_movement_journal};
use crate::payout_evidence::decode_payout_evidence;

pub struct CausalFundFlow<'a> {
    pub sequence: u64,
    pub p25_receipt: &'a [u8],
    pub p26_source: &'a [u8],
    pub p27_source: &'a [u8],
    pub billing_accounts: &'a BillingLedgerAccounts,
}

#[derive(Debug, Clone)]
pub struct CausalFundStream {
    predecessor: CausalPaymentStream,
    allocations: AuthorizedFundAllocationRegistry,
    transfers: AuthorizedFundTransferRegistry,
    ledger: Ledger,
    flow_count: usize,
}
impl CausalFundStream {
    #[must_use]
    pub const fn captures(&self) -> &cofi_payment_authorization::AuthorizedCaptureRegistry {
        self.predecessor.captures()
    }
    #[must_use]
    pub const fn payouts(&self) -> &cofi_payout_authorization::AuthorizedPayoutRegistry {
        self.predecessor.payouts()
    }
    #[must_use]
    pub const fn allocations(&self) -> &AuthorizedFundAllocationRegistry {
        &self.allocations
    }
    #[must_use]
    pub const fn transfers(&self) -> &AuthorizedFundTransferRegistry {
        &self.transfers
    }
    #[must_use]
    pub const fn ledger(&self) -> &Ledger {
        &self.ledger
    }
    #[must_use]
    pub const fn flow_count(&self) -> usize {
        self.flow_count
    }
}
fn verify_snapshot(
    ledger: &Ledger,
    expected: &Ledger,
    genesis: &Ledger,
    journal_ids: &BTreeSet<JournalEntryId>,
    account_ids: &BTreeSet<AccountId>,
) -> Result<(), CodecError> {
    if ledger.entry_count() != journal_ids.len() || expected.entry_count() != journal_ids.len() {
        return Err(CodecError::Replay(
            "historical journal set incomplete or augmented".into(),
        ));
    }
    for id in journal_ids {
        match (ledger.entry(id), expected.entry(id)) {
            (Some(a), Some(b)) if a == b => {}
            _ => return Err(CodecError::Replay("historical journal differs".into())),
        }
    }
    for id in account_ids {
        if genesis.account(id).is_none()
            || ledger.account(id) != genesis.account(id)
            || ledger.account(id) != expected.account(id)
            || ledger.balance(id) != expected.balance(id)
        {
            return Err(CodecError::Replay(
                "historical account/balance differs".into(),
            ));
        }
    }
    Ok(())
}

/// One nonempty staged stream of complete flows, with shared consumed authority
/// indexes across tenants. All P25 events are accepted first; then every P26;
/// then every P27. No arbitrary event interleavings, partial flows or refunds.
///
/// The supplied snapshots MUST be independently trustworthy for any external
/// claim. This module establishes only internal exact original-domain parity.
#[allow(clippy::too_many_arguments)]
pub fn rebuild_causal_fund_stream(
    flows: &[CausalFundFlow<'_>],
    community: &CommunityRegistry,
    genesis: &Ledger,
    after_p25: &Ledger,
    after_p26: &Ledger,
    after_p27: &Ledger,
) -> Result<CausalFundStream, CodecError> {
    if flows.is_empty() {
        return Err(CodecError::Replay(
            "empty original P26/P27 source stream".into(),
        ));
    }
    let payments: Vec<CausalPaymentFlow<'_>> = flows
        .iter()
        .map(|flow| CausalPaymentFlow {
            sequence: flow.sequence,
            p25_receipt: flow.p25_receipt,
            billing_accounts: flow.billing_accounts,
        })
        .collect();
    let predecessor = rebuild_causal_capture_payout_stream(&payments, genesis, after_p25)?;
    let mut ledger = predecessor.ledger().clone();
    let mut accounts: BTreeSet<AccountId> = BTreeSet::new();
    let mut ids: BTreeSet<JournalEntryId> = BTreeSet::new();
    for flow in flows {
        let p25 = decode_payout_evidence(flow.p25_receipt)?;
        let draft = p25
            .capture()
            .request()
            .authorized_finalization()
            .finalized()
            .draft();
        ids.insert(
            journal_entry_id_for_invoice(draft.invoice_id())
                .map_err(|e| CodecError::Replay(e.to_string()))?,
        );
        ids.insert(p25.capture().journal_entry_id().clone());
        ids.insert(p25.journal_entry_id().clone());
        accounts.insert(flow.billing_accounts.receivable().clone());
        accounts.insert(flow.billing_accounts.revenue().clone());
        accounts.insert(p25.capture().accounts().receivable().clone());
        accounts.insert(p25.capture().accounts().processor_clearing().clone());
        accounts.insert(p25.accounts().processor_clearing().clone());
        accounts.insert(p25.accounts().bank_cash().clone());
        if let Some(fee) = p25.accounts().processor_fee_expense() {
            accounts.insert(fee.clone());
        }
    }
    verify_snapshot(&ledger, after_p25, genesis, &ids, &accounts)?;
    let mut allocations = AuthorizedFundAllocationRegistry::new();
    for flow in flows {
        let source = decode_fund_movement(flow.p26_source)?;
        let FundMovementFact::Allocation { event, source_cash } = source else {
            return Err(CodecError::Replay(
                "P26 requires original allocation source".into(),
            ));
        };
        let expected_journal = verify_fund_movement_journal(
            &FundMovementFact::Allocation {
                event: event.clone(),
                source_cash: source_cash.clone(),
            },
            community,
            after_p26,
        )?;
        let p25 = decode_payout_evidence(flow.p25_receipt)?;
        let payout = predecessor
            .payouts()
            .authorization_for_event(p25.event().source_event_id())
            .ok_or_else(|| CodecError::Replay("previous P25 payout authority absent".into()))?
            .clone();
        let request = AuthorizedFundAllocationRequest::new(
            event.source_event_id().clone(),
            event.allocation_id().clone(),
            event.fund_id().clone(),
            payout,
            event.effective_at_unix_ms(),
            event.observed_at_unix_ms(),
        );
        let accepted = match allocations
            .apply(community, request, &mut ledger)
            .map_err(|e| CodecError::Replay(e.to_string()))?
        {
            AuthorizedFundAllocationOutcome::Created { authorization } => authorization,
            AuthorizedFundAllocationOutcome::Replayed { .. } => {
                return Err(CodecError::Replay(
                    "P26 original payout authority consumed twice".into(),
                ));
            }
        };
        if accepted.allocation_event() != &event
            || accepted.source_cash_account_id() != &source_cash
            || accepted.journal_entry_id() != &expected_journal
        {
            return Err(CodecError::Replay("P26 authority/source mismatch".into()));
        }
        if !ids.insert(expected_journal) {
            return Err(CodecError::Replay("P26 original journal duplicate".into()));
        }
        accounts.insert(source_cash);
        let fund = community
            .fund(event.fund_id())
            .ok_or_else(|| CodecError::Replay("P26 original fund absent".into()))?;
        accounts.insert(fund.ledger_account_id().clone());
    }
    if allocations.payout_allocation_count() != flows.len() {
        return Err(CodecError::Replay(
            "incomplete consumed P25 payout index".into(),
        ));
    }
    verify_snapshot(&ledger, after_p26, genesis, &ids, &accounts)?;
    let mut transfers = AuthorizedFundTransferRegistry::new();
    for flow in flows {
        let source = decode_fund_movement(flow.p27_source)?;
        let FundMovementFact::Transfer(event) = source else {
            return Err(CodecError::Replay(
                "P27 requires original fund-transfer source".into(),
            ));
        };
        let expected_journal = verify_fund_movement_journal(
            &FundMovementFact::Transfer(event.clone()),
            community,
            after_p27,
        )?;
        let previous = decode_fund_movement(flow.p26_source)?;
        let FundMovementFact::Allocation {
            event: allocation_event,
            ..
        } = previous
        else {
            return Err(CodecError::Replay("missing P26 original allocation".into()));
        };
        let original_allocation = allocations
            .authorization_for_event(allocation_event.source_event_id())
            .ok_or_else(|| CodecError::Replay("P26 original accepted allocation missing".into()))?
            .clone();
        let request = AuthorizedFundTransferRequest::new(
            event.source_event_id().clone(),
            event.transfer_id().clone(),
            event.destination_fund_id().clone(),
            original_allocation,
            event.effective_at_unix_ms(),
            event.observed_at_unix_ms(),
        );
        let accepted = match transfers
            .apply(community, request, &mut ledger)
            .map_err(|e| CodecError::Replay(e.to_string()))?
        {
            AuthorizedFundTransferOutcome::Created { authorization } => authorization,
            AuthorizedFundTransferOutcome::Replayed { .. } => {
                return Err(CodecError::Replay(
                    "P27 allocation authority consumed twice".into(),
                ));
            }
        };
        if accepted.transfer_event() != &event || accepted.journal_entry_id() != &expected_journal {
            return Err(CodecError::Replay(
                "P27 transfer authority/source mismatch".into(),
            ));
        }
        if !ids.insert(expected_journal) {
            return Err(CodecError::Replay("P27 original journal duplicate".into()));
        }
        for fund_id in [event.source_fund_id(), event.destination_fund_id()] {
            let fund = community
                .fund(fund_id)
                .ok_or_else(|| CodecError::Replay("P27 original fund absent".into()))?;
            accounts.insert(fund.ledger_account_id().clone());
        }
    }
    if transfers.allocation_transfer_count() != flows.len() {
        return Err(CodecError::Replay(
            "incomplete consumed P26 allocation index".into(),
        ));
    }
    verify_snapshot(&ledger, after_p27, genesis, &ids, &accounts)?;
    Ok(CausalFundStream {
        predecessor,
        allocations,
        transfers,
        ledger,
        flow_count: flows.len(),
    })
}
