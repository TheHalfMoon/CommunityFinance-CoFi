//! Bounded ordered multi-flow P23 -> P24 -> P25 causal reconstruction.
//! This reconstructs original capture/payout registries across multiple
//! complete, noninterleaved flows using a private account-only Ledger.
//! Caller-supplied order, receipts, genesis, and reference are NOT
//! independently authenticated or a production G001 admission certificate.

use std::collections::BTreeSet;

use cofi_billing::{
    BillingApplyOutcome, BillingLedgerAccounts, BillingLedgerBridge, journal_entry_id_for_invoice,
};
use cofi_ledger::{AccountId, JournalEntryId, Ledger};
use cofi_payment_authorization::{AuthorizedCaptureOutcome, AuthorizedCaptureRegistry};
use cofi_payout_authorization::{
    AuthorizedPayoutOutcome, AuthorizedPayoutRegistry, AuthorizedPayoutRequest,
};

use crate::CodecError;
use crate::payout_evidence::{decode_payout_evidence, verify_payout_journal};

pub struct CausalPaymentFlow<'a> {
    /// Untrusted contiguous 1-based caller-supplied flow ordinal.
    pub sequence: u64,
    pub p25_receipt: &'a [u8],
    pub billing_accounts: &'a BillingLedgerAccounts,
}

#[derive(Debug, Clone)]
pub struct CausalPaymentStream {
    captures: AuthorizedCaptureRegistry,
    payouts: AuthorizedPayoutRegistry,
    ledger: Ledger,
    flow_count: usize,
}

impl CausalPaymentStream {
    #[must_use]
    pub const fn captures(&self) -> &AuthorizedCaptureRegistry {
        &self.captures
    }
    #[must_use]
    pub const fn payouts(&self) -> &AuthorizedPayoutRegistry {
        &self.payouts
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

/// Rebuild multiple complete *noninterleaved* P23/P24/P25 flows, sharing
/// the ORIGINAL authorized capture/payout registries and one shadow Ledger.
/// Strict sequence numbers are a consistency test, NOT trusted source order.
/// Exact reference journal set and touched-account balances must agree.
pub fn rebuild_causal_capture_payout_stream(
    flows: &[CausalPaymentFlow<'_>],
    genesis: &Ledger,
    reference: &Ledger,
) -> Result<CausalPaymentStream, CodecError> {
    if flows.is_empty() || genesis.entry_count() != 0 {
        return Err(CodecError::Replay(
            "multiflow requires nonempty source and zero-journal genesis".into(),
        ));
    }
    let expected = flows
        .len()
        .checked_mul(3)
        .ok_or_else(|| CodecError::Replay("flow journal count overflow".into()))?;
    if reference.entry_count() != expected {
        return Err(CodecError::Replay(
            "incomplete or extra reference journals".into(),
        ));
    }
    let mut ledger = genesis.clone();
    let mut captures = AuthorizedCaptureRegistry::new();
    let mut payouts = AuthorizedPayoutRegistry::new();
    let mut seen: BTreeSet<JournalEntryId> = BTreeSet::new();
    let mut accounts: BTreeSet<AccountId> = BTreeSet::new();
    for (index, flow) in flows.iter().enumerate() {
        let ordinal = u64::try_from(index)
            .ok()
            .and_then(|n| n.checked_add(1))
            .ok_or_else(|| CodecError::Replay("source sequence overflow".into()))?;
        if flow.sequence != ordinal {
            return Err(CodecError::Replay(
                "source sequence missing, duplicated, or out of order".into(),
            ));
        }
        let source = decode_payout_evidence(flow.p25_receipt)?;
        verify_payout_journal(&source, reference)?;
        let capture_request = source.capture().request();
        let invoice = capture_request
            .authorized_finalization()
            .finalized()
            .to_billing_event();
        let invoice_id = journal_entry_id_for_invoice(invoice.invoice_id())
            .map_err(|e| CodecError::Replay(e.to_string()))?;
        let capture_id = source.capture().journal_entry_id().clone();
        let payout_id = source.journal_entry_id().clone();
        for id in [&invoice_id, &capture_id, &payout_id] {
            if !seen.insert(id.clone()) {
                return Err(CodecError::Replay("accepted journal consumed twice".into()));
            }
        }
        match BillingLedgerBridge::new()
            .apply(&invoice, flow.billing_accounts, &mut ledger)
            .map_err(|e| CodecError::Replay(e.to_string()))?
        {
            BillingApplyOutcome::Committed { journal_entry_id }
                if journal_entry_id == invoice_id => {}
            _ => return Err(CodecError::Replay("invoice must be newly committed".into())),
        }
        let authorized_capture = match captures
            .capture(
                capture_request.clone(),
                source.capture().accounts(),
                &mut ledger,
            )
            .map_err(|e| CodecError::Replay(e.to_string()))?
        {
            AuthorizedCaptureOutcome::Created { authorization } => authorization,
            AuthorizedCaptureOutcome::Replayed { .. } => {
                return Err(CodecError::Replay(
                    "capture authority consumed twice".into(),
                ));
            }
        };
        if authorized_capture.journal_entry_id() != &capture_id {
            return Err(CodecError::Replay("original P24 journal mismatch".into()));
        }
        let payout_event = source.event();
        let request = AuthorizedPayoutRequest::new(
            payout_event.source_event_id().clone(),
            payout_event.payout_id().clone(),
            payout_event.bank_transaction_reference().clone(),
            authorized_capture,
            payout_event.processor_fee_minor(),
            payout_event.net_amount_minor(),
            payout_event.paid_at_unix_ms(),
            payout_event.observed_at_unix_ms(),
        );
        let authorized_payout = match payouts
            .apply(request, source.accounts(), &mut ledger)
            .map_err(|e| CodecError::Replay(e.to_string()))?
        {
            AuthorizedPayoutOutcome::Created { authorization } => authorization,
            AuthorizedPayoutOutcome::Replayed { .. } => {
                return Err(CodecError::Replay("payout authority consumed twice".into()));
            }
        };
        if authorized_payout.journal_entry_id() != &payout_id
            || authorized_payout.payout_event() != payout_event
        {
            return Err(CodecError::Replay(
                "original P25 source or journal mismatch".into(),
            ));
        }
        for id in [&invoice_id, &capture_id, &payout_id] {
            match (ledger.entry(id), reference.entry(id)) {
                (Some(actual), Some(expected)) if actual == expected => {}
                _ => {
                    return Err(CodecError::Replay(
                        "reconstructed journal differs from reference".into(),
                    ));
                }
            }
        }
        accounts.insert(flow.billing_accounts.receivable().clone());
        accounts.insert(flow.billing_accounts.revenue().clone());
        accounts.insert(source.capture().accounts().receivable().clone());
        accounts.insert(source.capture().accounts().processor_clearing().clone());
        accounts.insert(source.accounts().processor_clearing().clone());
        accounts.insert(source.accounts().bank_cash().clone());
        if let Some(fee) = source.accounts().processor_fee_expense() {
            accounts.insert(fee.clone());
        }
    }
    if ledger.entry_count() != expected || seen.len() != expected {
        return Err(CodecError::Replay("accepted journal count mismatch".into()));
    }
    for account in accounts {
        if genesis.account(&account).is_none()
            || genesis.account(&account) != reference.account(&account)
            || ledger.account(&account) != reference.account(&account)
            || ledger.balance(&account) != reference.balance(&account)
        {
            return Err(CodecError::Replay(
                "accepted account/balance mismatch".into(),
            ));
        }
    }
    Ok(CausalPaymentStream {
        captures,
        payouts,
        ledger,
        flow_count: flows.len(),
    })
}
