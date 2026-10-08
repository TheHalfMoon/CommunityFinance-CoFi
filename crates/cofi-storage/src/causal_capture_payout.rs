//! Bounded original P23 invoice → P24 capture → P25 payout CAUSAL replay.
//! Reconstructs the actual authorized registries through their original
//! Committed transitions, ONLY on a private caller-provided clean genesis
//! Ledger, then checks all three journals against a reference final Ledger.
//! Neither input is independently authenticated by this module.

use crate::CodecError;
use crate::payout_evidence::{decode_payout_evidence, verify_payout_journal};
use cofi_billing::{
    BillingApplyOutcome, BillingLedgerAccounts, BillingLedgerBridge, journal_entry_id_for_invoice,
};
use cofi_ledger::{AccountId, Ledger};
use cofi_payment_authorization::{AuthorizedCaptureOutcome, AuthorizedCaptureRegistry};
use cofi_payout_authorization::{
    AuthorizedPayoutOutcome, AuthorizedPayoutRegistry, AuthorizedPayoutRequest,
};
use std::collections::BTreeSet;

#[derive(Debug, Clone)]
pub struct CausalCapturePayout {
    captures: AuthorizedCaptureRegistry,
    payouts: AuthorizedPayoutRegistry,
    ledger: Ledger,
}
impl CausalCapturePayout {
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
}

/// Rebuilds EXACTLY ONE P23→P24→P25 causally complete accepted flow.
/// Rejects any genesis that already carries a historical journal: using a
/// fully hydrated Ledger would bypass the original "Committed" admission
/// requirements in both original authorization registries.
///
/// The caller must independently provide the original account-only genesis
/// and the trusted exact final Ledger, whose three expected journals must
/// match fully and have no extra entries. This does NOT prove their trust.
pub fn rebuild_causal_capture_payout(
    p25_receipt: &[u8],
    billing_accounts: &BillingLedgerAccounts,
    genesis: &Ledger,
    reference: &Ledger,
) -> Result<CausalCapturePayout, CodecError> {
    if genesis.entry_count() != 0 {
        return Err(CodecError::Replay(
            "causal P24/P25 replay requires zero-journal genesis".into(),
        ));
    }
    let payout = decode_payout_evidence(p25_receipt)?;
    verify_payout_journal(&payout, reference)?;
    let capture_request = payout.capture().request();
    let invoice = capture_request
        .authorized_finalization()
        .finalized()
        .to_billing_event();
    let invoice_id = journal_entry_id_for_invoice(invoice.invoice_id())
        .map_err(|e| CodecError::Replay(e.to_string()))?;
    let captured_id = payout.capture().journal_entry_id().clone();
    let paid_id = payout.journal_entry_id().clone();

    let mut ledger = genesis.clone();
    match BillingLedgerBridge::new()
        .apply(&invoice, billing_accounts, &mut ledger)
        .map_err(|e| CodecError::Replay(e.to_string()))?
    {
        BillingApplyOutcome::Committed { journal_entry_id } if journal_entry_id == invoice_id => {}
        _ => {
            return Err(CodecError::Replay(
                "original P23 invoice not newly committed from genesis".into(),
            ));
        }
    }

    let mut captures = AuthorizedCaptureRegistry::new();
    let capture = match captures
        .capture(
            capture_request.clone(),
            payout.capture().accounts(),
            &mut ledger,
        )
        .map_err(|e| CodecError::Replay(e.to_string()))?
    {
        AuthorizedCaptureOutcome::Created { authorization } => authorization,
        AuthorizedCaptureOutcome::Replayed { .. } => {
            return Err(CodecError::Replay(
                "P24 authorization unexpectedly replayed".into(),
            ));
        }
    };
    if capture.journal_entry_id() != &captured_id {
        return Err(CodecError::Replay(
            "original P24 source/journal diverged".into(),
        ));
    }

    let event = payout.event();
    let request = AuthorizedPayoutRequest::new(
        event.source_event_id().clone(),
        event.payout_id().clone(),
        event.bank_transaction_reference().clone(),
        capture,
        event.processor_fee_minor(),
        event.net_amount_minor(),
        event.paid_at_unix_ms(),
        event.observed_at_unix_ms(),
    );
    let mut payouts = AuthorizedPayoutRegistry::new();
    let result = match payouts
        .apply(request, payout.accounts(), &mut ledger)
        .map_err(|e| CodecError::Replay(e.to_string()))?
    {
        AuthorizedPayoutOutcome::Created { authorization } => authorization,
        AuthorizedPayoutOutcome::Replayed { .. } => {
            return Err(CodecError::Replay(
                "P25 authorization unexpectedly replayed".into(),
            ));
        }
    };
    if result.journal_entry_id() != &paid_id || result.payout_event() != event {
        return Err(CodecError::Replay(
            "P25 newly authorized result differs from checked source".into(),
        ));
    }

    if ledger.entry_count() != 3 || reference.entry_count() != 3 {
        return Err(CodecError::Replay(
            "historical reference must contain exactly P23/P24/P25 journals".into(),
        ));
    }
    for id in [&invoice_id, &captured_id, &paid_id] {
        match (ledger.entry(id), reference.entry(id)) {
            (Some(a), Some(b)) if a == b => {}
            _ => {
                return Err(CodecError::Replay(
                    "reconstructed original journal differs from reference".into(),
                ));
            }
        }
    }

    // All known involved account definitions and running balances must match
    // the independently supplied reference and the exact clean genesis.
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
    for id in accounts {
        if genesis.account(&id).is_none()
            || ledger.account(&id) != reference.account(&id)
            || genesis.account(&id) != reference.account(&id)
            || ledger.balance(&id) != reference.balance(&id)
        {
            return Err(CodecError::Replay(
                "historical account or balance differs from reference".into(),
            ));
        }
    }

    Ok(CausalCapturePayout {
        captures,
        payouts,
        ledger,
    })
}
