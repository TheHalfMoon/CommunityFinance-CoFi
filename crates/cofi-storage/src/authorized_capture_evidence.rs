//! P24 capture *source evidence*, not a reconstructed P24 authorization.
//! The actual AuthorizedCaptureRegistry::capture requires a causal pre-capture
//! ledger and a new Committed journal. We do NOT run it on a fully hydrated
//! Ledger or synthesize AuthorizedCapture from JSON.

use crate::authorized_finalization::decode_authorized_finalization;
use crate::{CodecError, parse_i64_exact};
use cofi_ledger::{AccountId, JournalEntryId, Ledger};
use cofi_payment_authorization::AuthorizedCaptureRequest;
use cofi_payments::{
    ConnectorTransactionId, PaymentApplyOutcome, PaymentEvent, PaymentEventId, PaymentId,
    PaymentLedgerAccounts, PaymentLedgerBridge, PaymentStatus, journal_entry_id_for_payment,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const VERSION: u64 = 1;
const KIND: &str = "authorized.capture_evidence";
const MAX_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureEvidence {
    request: AuthorizedCaptureRequest,
    accounts: PaymentLedgerAccounts,
    journal_entry_id: JournalEntryId,
}
impl CaptureEvidence {
    #[must_use]
    pub const fn request(&self) -> &AuthorizedCaptureRequest {
        &self.request
    }
    #[must_use]
    pub const fn accounts(&self) -> &PaymentLedgerAccounts {
        &self.accounts
    }
    #[must_use]
    pub const fn journal_entry_id(&self) -> &JournalEntryId {
        &self.journal_entry_id
    }
    fn original_payment_event(&self) -> PaymentEvent {
        let draft = self.request.authorized_finalization().finalized().draft();
        PaymentEvent::new(
            self.request.source_event_id().clone(),
            draft.organization_scope().clone(),
            draft.invoice_id().clone(),
            self.request.payment_id().clone(),
            self.request.connector_transaction_id().clone(),
            PaymentStatus::Charged,
            draft.currency(),
            draft.total_minor(),
            Some(self.request.captured_at_unix_ms()),
            self.request.observed_at_unix_ms(),
        )
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
struct CaptureRecord {
    source_event_id: String,
    payment_id: String,
    connector_transaction_id: String,
    authorized_finalization: serde_json::Value,
    captured_at_unix_ms: String,
    observed_at_unix_ms: String,
    receivable_account_id: String,
    processor_clearing_account_id: String,
    expected_journal_entry_id: String,
}
fn invalid<E: std::fmt::Display>(e: E) -> CodecError {
    CodecError::InvalidDomain(e.to_string())
}
fn checked(record: CaptureRecord) -> Result<CaptureEvidence, CodecError> {
    let p23 = serde_json::to_vec(&record.authorized_finalization)
        .map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    let finalized = decode_authorized_finalization(&p23)?;
    let payment_id = PaymentId::new(record.payment_id).map_err(invalid)?;
    let expected_journal = journal_entry_id_for_payment(&payment_id).map_err(invalid)?;
    if expected_journal.as_str() != record.expected_journal_entry_id {
        return Err(CodecError::Replay(
            "payment journal identity differs from original source".into(),
        ));
    }
    let request = AuthorizedCaptureRequest::new(
        PaymentEventId::new(record.source_event_id).map_err(invalid)?,
        payment_id,
        ConnectorTransactionId::new(record.connector_transaction_id).map_err(invalid)?,
        finalized,
        parse_i64_exact(&record.captured_at_unix_ms)?,
        parse_i64_exact(&record.observed_at_unix_ms)?,
    );
    let accounts = PaymentLedgerAccounts::new(
        AccountId::new(record.receivable_account_id).map_err(invalid)?,
        AccountId::new(record.processor_clearing_account_id).map_err(invalid)?,
    )
    .map_err(invalid)?;
    Ok(CaptureEvidence {
        request,
        accounts,
        journal_entry_id: expected_journal,
    })
}
/// Record P24 source facts supplied with the original fully checked P23
/// authorization receipt, never arbitrary unchecked finalization fields.
pub fn encode_capture_evidence(
    request: &AuthorizedCaptureRequest,
    p23_receipt: &[u8],
    accounts: &PaymentLedgerAccounts,
) -> Result<Vec<u8>, CodecError> {
    let actual_p23 = decode_authorized_finalization(p23_receipt)?;
    if request.authorized_finalization() != &actual_p23 {
        return Err(CodecError::Replay(
            "P24 does not match checked P23 accepted ancestry".into(),
        ));
    }
    let journal = journal_entry_id_for_payment(request.payment_id()).map_err(invalid)?;
    let p23 = serde_json::from_slice(p23_receipt)
        .map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    let record = CaptureRecord {
        source_event_id: request.source_event_id().as_str().to_owned(),
        payment_id: request.payment_id().as_str().to_owned(),
        connector_transaction_id: request.connector_transaction_id().as_str().to_owned(),
        authorized_finalization: p23,
        captured_at_unix_ms: request.captured_at_unix_ms().to_string(),
        observed_at_unix_ms: request.observed_at_unix_ms().to_string(),
        receivable_account_id: accounts.receivable().as_str().to_owned(),
        processor_clearing_account_id: accounts.processor_clearing().as_str().to_owned(),
        expected_journal_entry_id: journal.as_str().to_owned(),
    };
    let bytes = serde_json::to_vec(&Envelope {
        schema_version: VERSION,
        record_type: KIND.into(),
        payload: record,
    })
    .map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    if bytes.len() > MAX_BYTES {
        return Err(CodecError::InvalidPayload(
            "capture evidence exceeds 1MiB".into(),
        ));
    }
    Ok(bytes)
}
pub fn decode_capture_evidence(bytes: &[u8]) -> Result<CaptureEvidence, CodecError> {
    if bytes.len() > MAX_BYTES {
        return Err(CodecError::InvalidPayload(
            "capture evidence exceeds 1MiB".into(),
        ));
    }
    let envelope: Envelope<CaptureRecord> =
        serde_json::from_slice(bytes).map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    if envelope.schema_version != VERSION {
        return Err(CodecError::UnsupportedVersion(envelope.schema_version));
    }
    if envelope.record_type != KIND {
        return Err(CodecError::UnsupportedRecordKind(envelope.record_type));
    }
    checked(envelope.payload)
}
/// Verify actual P05 source/journal parity only on a PRIVATE Ledger clone,
/// requiring Replayed and refusing Committed or IgnoredNoCapture.
pub fn verify_capture_evidence_journal(
    fact: &CaptureEvidence,
    ledger: &Ledger,
) -> Result<JournalEntryId, CodecError> {
    let mut shadow = ledger.clone();
    let outcome = PaymentLedgerBridge::new()
        .apply(&fact.original_payment_event(), fact.accounts(), &mut shadow)
        .map_err(|e| CodecError::Replay(e.to_string()))?;
    match outcome {
        PaymentApplyOutcome::Replayed { journal_entry_id }
            if journal_entry_id == fact.journal_entry_id =>
        {
            Ok(journal_entry_id)
        }
        _ => Err(CodecError::Replay(
            "original payment journal missing, changed or not captured".into(),
        )),
    }
}
/// Immutable typed P24 evidence key and consumed payment ID checks.
/// This is not an AuthorizedCaptureRegistry recovery.
pub fn verify_capture_evidence_history<'a>(
    receipts: impl IntoIterator<Item = &'a [u8]>,
    ledger: &Ledger,
) -> Result<usize, CodecError> {
    let mut seen: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut payment_to_source: BTreeMap<String, String> = BTreeMap::new();
    for bytes in receipts {
        let fact = decode_capture_evidence(bytes)?;
        let source = fact.request.source_event_id().as_str().to_owned();
        let payment = fact.request.payment_id().as_str().to_owned();
        let canonical = serde_json::to_vec(
            &serde_json::from_slice::<Envelope<CaptureRecord>>(bytes)
                .map_err(|e| CodecError::InvalidPayload(e.to_string()))?,
        )
        .map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
        if let Some(previous) = seen.get(&source) {
            if previous != &canonical {
                return Err(CodecError::Replay("source event was changed".into()));
            }
            continue;
        }
        if let Some(previous) = payment_to_source.get(&payment) {
            if previous != &source {
                return Err(CodecError::Replay(
                    "payment consumed by two capture events".into(),
                ));
            }
        }
        verify_capture_evidence_journal(&fact, ledger)?;
        seen.insert(source.clone(), canonical);
        payment_to_source.insert(payment, source);
    }
    Ok(seen.len())
}
