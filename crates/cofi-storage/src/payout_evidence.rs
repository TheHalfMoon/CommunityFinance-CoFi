//! P25 payout source-to-journal parity, NOT recovered payout authorization.
//! Original payout/fee computations remain entirely inside PayoutLedgerBridge.
//! The input Ledger and source receipt must still be independently trusted.

use crate::authorized_capture_evidence::{
    CaptureEvidence, decode_capture_evidence, verify_capture_evidence_journal,
};
use crate::{CodecError, parse_i64_exact, parse_i128_exact};
use cofi_ledger::{AccountId, Currency, JournalEntryId, Ledger, LedgerScopeId};
use cofi_payments::{
    BankTransactionReference, PaymentId, PayoutApplyOutcome, PayoutLedgerAccounts,
    PayoutLedgerBridge, ProcessorPayoutEvent, ProcessorPayoutEventId, ProcessorPayoutId,
    journal_entry_id_for_payout,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const VERSION: u64 = 1;
const KIND: &str = "authorized.payout_evidence";
const MAX_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayoutEvidence {
    event: ProcessorPayoutEvent,
    accounts: PayoutLedgerAccounts,
    capture: CaptureEvidence,
    journal_entry_id: JournalEntryId,
}
impl PayoutEvidence {
    #[must_use]
    pub const fn event(&self) -> &ProcessorPayoutEvent {
        &self.event
    }
    #[must_use]
    pub const fn accounts(&self) -> &PayoutLedgerAccounts {
        &self.accounts
    }
    #[must_use]
    pub const fn capture(&self) -> &CaptureEvidence {
        &self.capture
    }
    #[must_use]
    pub const fn journal_entry_id(&self) -> &JournalEntryId {
        &self.journal_entry_id
    }
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope<T> {
    schema_version: u64,
    record_type: String,
    payload: T,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PayoutRecord {
    source_event_id: String,
    organization_scope: String,
    payment_id: String,
    payout_id: String,
    bank_transaction_reference: String,
    currency: String,
    gross_amount_minor: String,
    processor_fee_minor: String,
    net_amount_minor: String,
    paid_at_unix_ms: String,
    observed_at_unix_ms: String,
    processor_clearing_account_id: String,
    bank_cash_account_id: String,
    processor_fee_expense_account_id: Option<String>,
    p24_capture: serde_json::Value,
    expected_journal_entry_id: String,
}
fn invalid<E: std::fmt::Display>(e: E) -> CodecError {
    CodecError::InvalidDomain(e.to_string())
}
fn checked(r: PayoutRecord) -> Result<PayoutEvidence, CodecError> {
    let p24_bytes = serde_json::to_vec(&r.p24_capture)
        .map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    let capture = decode_capture_evidence(&p24_bytes)?;
    let payment_id = PaymentId::new(r.payment_id).map_err(invalid)?;
    let payout_id = ProcessorPayoutId::new(r.payout_id).map_err(invalid)?;
    let journal_entry_id = journal_entry_id_for_payout(&payout_id).map_err(invalid)?;
    if r.expected_journal_entry_id != journal_entry_id.as_str() {
        return Err(CodecError::Replay(
            "payout journal identity disagrees with source".into(),
        ));
    }
    let gross = parse_i128_exact(&r.gross_amount_minor)?;
    let fee = parse_i128_exact(&r.processor_fee_minor)?;
    let net = parse_i128_exact(&r.net_amount_minor)?;
    let capture_request = capture.request();
    let draft = capture_request
        .authorized_finalization()
        .finalized()
        .draft();
    if payment_id != *capture_request.payment_id()
        || r.organization_scope != draft.organization_scope().as_str()
        || r.currency != draft.currency().code()
        || gross != draft.total_minor()
        || gross <= 0
        || fee < 0
        || net <= 0
        || net.checked_add(fee) != Some(gross)
        || r.processor_clearing_account_id != capture.accounts().processor_clearing().as_str()
    {
        return Err(CodecError::Replay(
            "P25 source disagrees with P24 capture or amounts".into(),
        ));
    }
    let event = ProcessorPayoutEvent::new(
        ProcessorPayoutEventId::new(r.source_event_id).map_err(invalid)?,
        LedgerScopeId::new(r.organization_scope).map_err(invalid)?,
        payment_id,
        payout_id,
        BankTransactionReference::new(r.bank_transaction_reference).map_err(invalid)?,
        Currency::new(&r.currency).map_err(invalid)?,
        gross,
        fee,
        net,
        parse_i64_exact(&r.paid_at_unix_ms)?,
        parse_i64_exact(&r.observed_at_unix_ms)?,
    );
    if event.paid_at_unix_ms() < capture_request.captured_at_unix_ms() {
        return Err(CodecError::Replay(
            "payout predates checked original capture".into(),
        ));
    }
    let fee_account = r
        .processor_fee_expense_account_id
        .map(|a| AccountId::new(a).map_err(invalid))
        .transpose()?;
    let accounts = PayoutLedgerAccounts::new(
        AccountId::new(r.processor_clearing_account_id).map_err(invalid)?,
        AccountId::new(r.bank_cash_account_id).map_err(invalid)?,
        fee_account,
    )
    .map_err(invalid)?;
    Ok(PayoutEvidence {
        event,
        accounts,
        capture,
        journal_entry_id,
    })
}
fn source_record(
    event: &ProcessorPayoutEvent,
    accounts: &PayoutLedgerAccounts,
    capture: serde_json::Value,
) -> Result<PayoutRecord, CodecError> {
    let journal = journal_entry_id_for_payout(event.payout_id()).map_err(invalid)?;
    Ok(PayoutRecord {
        source_event_id: event.source_event_id().as_str().to_owned(),
        organization_scope: event.organization_scope().as_str().to_owned(),
        payment_id: event.payment_id().as_str().to_owned(),
        payout_id: event.payout_id().as_str().to_owned(),
        bank_transaction_reference: event.bank_transaction_reference().as_str().to_owned(),
        currency: event.currency().code().to_owned(),
        gross_amount_minor: event.gross_amount_minor().to_string(),
        processor_fee_minor: event.processor_fee_minor().to_string(),
        net_amount_minor: event.net_amount_minor().to_string(),
        paid_at_unix_ms: event.paid_at_unix_ms().to_string(),
        observed_at_unix_ms: event.observed_at_unix_ms().to_string(),
        processor_clearing_account_id: accounts.processor_clearing().as_str().to_owned(),
        bank_cash_account_id: accounts.bank_cash().as_str().to_owned(),
        processor_fee_expense_account_id: accounts
            .processor_fee_expense()
            .map(|x| x.as_str().to_owned()),
        p24_capture: capture,
        expected_journal_entry_id: journal.as_str().to_owned(),
    })
}
pub fn encode_payout_evidence(
    event: &ProcessorPayoutEvent,
    accounts: &PayoutLedgerAccounts,
    p24_receipt: &[u8],
) -> Result<Vec<u8>, CodecError> {
    // This validates the P24 accepted-history receipt, but NOT its external
    // authenticity or the complete financial stream cutoff.
    decode_capture_evidence(p24_receipt)?;
    let p24 = serde_json::from_slice(p24_receipt)
        .map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    let record = source_record(event, accounts, p24)?;
    let bytes = serde_json::to_vec(&Envelope {
        schema_version: VERSION,
        record_type: KIND.to_owned(),
        payload: record,
    })
    .map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    if bytes.len() > MAX_BYTES {
        return Err(CodecError::InvalidPayload(
            "payout receipt exceeds 1MiB".into(),
        ));
    }
    let recovered = decode_payout_evidence(&bytes)?;
    if recovered.event() != event || recovered.accounts() != accounts {
        return Err(CodecError::Replay(
            "P25 source is not canonical for checked P24 ancestry".into(),
        ));
    }
    Ok(bytes)
}
pub fn decode_payout_evidence(bytes: &[u8]) -> Result<PayoutEvidence, CodecError> {
    if bytes.len() > MAX_BYTES {
        return Err(CodecError::InvalidPayload(
            "payout receipt exceeds 1MiB".into(),
        ));
    }
    let envelope: Envelope<PayoutRecord> =
        serde_json::from_slice(bytes).map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    if envelope.schema_version != VERSION {
        return Err(CodecError::UnsupportedVersion(envelope.schema_version));
    }
    if envelope.record_type != KIND {
        return Err(CodecError::UnsupportedRecordKind(envelope.record_type));
    }
    checked(envelope.payload)
}
/// Verify P24 payment lineage first, then the original P05/P25 ledger
/// journals on a PRIVATE clone. Only exact preexisting Replayed is accepted.
pub fn verify_payout_journal(
    fact: &PayoutEvidence,
    ledger: &Ledger,
) -> Result<JournalEntryId, CodecError> {
    verify_capture_evidence_journal(fact.capture(), ledger)?;
    let mut shadow = ledger.clone();
    match PayoutLedgerBridge::new()
        .apply(fact.event(), fact.accounts(), &mut shadow)
        .map_err(|e| CodecError::Replay(e.to_string()))?
    {
        PayoutApplyOutcome::Replayed { journal_entry_id }
            if journal_entry_id == fact.journal_entry_id =>
        {
            Ok(journal_entry_id)
        }
        _ => Err(CodecError::Replay(
            "original payout journal missing or changed".into(),
        )),
    }
}
/// Typed source, payout business key and single-full-payout payment binding
/// checks, not a reconstructed AuthorizedPayoutRegistry.
pub fn verify_payout_evidence_history<'a>(
    receipts: impl IntoIterator<Item = &'a [u8]>,
    ledger: &Ledger,
) -> Result<usize, CodecError> {
    let mut sources: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut payout_to_source: BTreeMap<String, String> = BTreeMap::new();
    let mut payment_to_payout: BTreeMap<String, String> = BTreeMap::new();
    for receipt in receipts {
        let fact = decode_payout_evidence(receipt)?;
        let source = fact.event.source_event_id().as_str().to_owned();
        let payout = fact.event.payout_id().as_str().to_owned();
        let payment = fact.event.payment_id().as_str().to_owned();
        let envelope: Envelope<PayoutRecord> = serde_json::from_slice(receipt)
            .map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
        let canonical =
            serde_json::to_vec(&envelope).map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
        if let Some(existing) = sources.get(&source) {
            if existing != &canonical {
                return Err(CodecError::Replay("changed P25 source identity".into()));
            }
            continue;
        }
        if let Some(existing) = payout_to_source.get(&payout) {
            if existing != &source {
                return Err(CodecError::Replay("payout business ID reused".into()));
            }
        }
        if let Some(existing) = payment_to_payout.get(&payment) {
            if existing != &payout {
                return Err(CodecError::Replay("payment paid out twice".into()));
            }
        }
        verify_payout_journal(&fact, ledger)?;
        sources.insert(source.clone(), canonical);
        payout_to_source.insert(payout.clone(), source);
        payment_to_payout.insert(payment, payout);
    }
    Ok(sources.len())
}
