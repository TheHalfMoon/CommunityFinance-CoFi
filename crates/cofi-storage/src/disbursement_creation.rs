//! Checked original disbursement-creation source and spend-bound replay.
//!
//! Only the original DisbursementEngine::create may derive Ready state and
//! consumed spend bindings. Neither provider calls nor journal posting occur.
//! Ledger, governance and source histories must be independently authenticated
//! and complete before this bounded adapter could be admitted to production.

use cofi_community::CommunityRegistry;
use cofi_disbursements::{
    BeneficiaryReference, DestinationReference, DisbursementCreation, DisbursementEngine,
    DisbursementEventId, DisbursementId,
};
use cofi_governance::GovernanceEngine;
use cofi_ledger::Ledger;
use serde::{Deserialize, Serialize};

use crate::governance_fund_spend::decode_governance_fund_spend;
use crate::{CodecError, parse_i64_exact};

const VERSION: u64 = 1;
const KIND: &str = "disbursement.creation";
const MAX_BYTES: usize = 1024 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope<T> {
    schema_version: u64,
    record_type: String,
    payload: T,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreationRecord {
    source_event_id: String,
    id: String,
    beneficiary_reference: String,
    destination_reference: String,
    created_at_unix_ms: String,
}

fn invalid<E: std::fmt::Display>(e: E) -> CodecError {
    CodecError::InvalidDomain(e.to_string())
}

impl CreationRecord {
    fn from_original(creation: &DisbursementCreation) -> Self {
        Self {
            source_event_id: creation.source_event_id().as_str().to_owned(),
            id: creation.id().as_str().to_owned(),
            beneficiary_reference: creation.beneficiary_reference().as_str().to_owned(),
            destination_reference: creation.destination_reference().as_str().to_owned(),
            created_at_unix_ms: creation.created_at_unix_ms().to_string(),
        }
    }

    fn checked(self) -> Result<DisbursementCreation, CodecError> {
        Ok(DisbursementCreation::new(
            DisbursementEventId::new(self.source_event_id).map_err(invalid)?,
            DisbursementId::new(self.id).map_err(invalid)?,
            BeneficiaryReference::new(self.beneficiary_reference).map_err(invalid)?,
            DestinationReference::new(self.destination_reference).map_err(invalid)?,
            parse_i64_exact(&self.created_at_unix_ms)?,
        ))
    }
}

pub fn encode_disbursement_creation(
    creation: &DisbursementCreation,
) -> Result<Vec<u8>, CodecError> {
    let bytes = serde_json::to_vec(&Envelope {
        schema_version: VERSION,
        record_type: KIND.to_owned(),
        payload: CreationRecord::from_original(creation),
    })
    .map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    if bytes.len() > MAX_BYTES {
        return Err(CodecError::InvalidPayload(
            "disbursement creation exceeds 1MiB".into(),
        ));
    }
    if decode_disbursement_creation(&bytes)? != *creation {
        return Err(CodecError::Replay(
            "disbursement creation source roundtrip differs".into(),
        ));
    }
    Ok(bytes)
}

pub fn decode_disbursement_creation(bytes: &[u8]) -> Result<DisbursementCreation, CodecError> {
    if bytes.len() > MAX_BYTES {
        return Err(CodecError::InvalidPayload(
            "disbursement creation exceeds 1MiB".into(),
        ));
    }
    let record: Envelope<CreationRecord> =
        serde_json::from_slice(bytes).map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    if record.schema_version != VERSION {
        return Err(CodecError::UnsupportedVersion(record.schema_version));
    }
    if record.record_type != KIND {
        return Err(CodecError::UnsupportedRecordKind(record.record_type));
    }
    record.payload.checked()
}

/// Reconstruct original disbursement creation and spend consumption indexes
/// from checked creation facts paired with their original governed spend facts.
/// Exact duplicates follow original domain idempotency. Missing or changed
/// original spend/journal ancestry is rejected by DisbursementEngine::create.
pub fn replay_disbursement_creations<'a>(
    facts: impl IntoIterator<Item = (&'a [u8], &'a [u8])>,
    community: &CommunityRegistry,
    governance: &GovernanceEngine,
    ledger: &Ledger,
) -> Result<DisbursementEngine, CodecError> {
    let mut engine = DisbursementEngine::new();
    for (creation_record, spend_record) in facts {
        let creation = decode_disbursement_creation(creation_record)?;
        let spend = decode_governance_fund_spend(spend_record)?;
        engine
            .create(community, governance, ledger, &spend, creation)
            .map_err(|e| CodecError::Replay(e.to_string()))?;
    }
    Ok(engine)
}
