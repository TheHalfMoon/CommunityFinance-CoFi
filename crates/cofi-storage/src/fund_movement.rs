//! Checked immutable allocation/transfer source facts with journal parity.
//! Verification runs the original bridge on a PRIVATE CLONE of the Ledger.
//! Only Replayed is accepted, never new Committed financial mutations.
use crate::{CodecError, parse_i64_exact, parse_i128_exact};
use cofi_community::{
    CommunityRegistry, FundAllocationAccounts, FundAllocationBridge, FundAllocationEvent,
    FundAllocationEventId, FundAllocationId, FundAllocationOutcome, FundId, FundTransferBridge,
    FundTransferEvent, FundTransferEventId, FundTransferId, FundTransferOutcome,
};
use cofi_ledger::{AccountId, Currency, JournalEntryId, Ledger, LedgerScopeId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const VERSION: u64 = 1;
const KIND: &str = "community.fund_movement";
const MAX_BYTES: usize = 1024 * 1024;
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FundMovementFact {
    Allocation {
        event: FundAllocationEvent,
        source_cash: AccountId,
    },
    Transfer(FundTransferEvent),
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope<T> {
    schema_version: u64,
    record_type: String,
    payload: T,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Record {
    Allocation {
        source_event_id: String,
        organization_scope: String,
        fund_id: String,
        allocation_id: String,
        source_cash: String,
        currency: String,
        amount_minor: String,
        effective_at_unix_ms: String,
        observed_at_unix_ms: String,
    },
    Transfer {
        source_event_id: String,
        organization_scope: String,
        source_fund_id: String,
        destination_fund_id: String,
        transfer_id: String,
        currency: String,
        amount_minor: String,
        effective_at_unix_ms: String,
        observed_at_unix_ms: String,
    },
}
fn invalid<E: std::fmt::Display>(error: E) -> CodecError {
    CodecError::InvalidDomain(error.to_string())
}
impl Record {
    fn from_fact(f: &FundMovementFact) -> Self {
        match f {
            FundMovementFact::Allocation {
                event: e,
                source_cash,
            } => Self::Allocation {
                source_event_id: e.source_event_id().as_str().to_owned(),
                organization_scope: e.organization_scope().as_str().to_owned(),
                fund_id: e.fund_id().as_str().to_owned(),
                allocation_id: e.allocation_id().as_str().to_owned(),
                source_cash: source_cash.as_str().to_owned(),
                currency: e.currency().code().to_owned(),
                amount_minor: e.amount_minor().to_string(),
                effective_at_unix_ms: e.effective_at_unix_ms().to_string(),
                observed_at_unix_ms: e.observed_at_unix_ms().to_string(),
            },
            FundMovementFact::Transfer(e) => Self::Transfer {
                source_event_id: e.source_event_id().as_str().to_owned(),
                organization_scope: e.organization_scope().as_str().to_owned(),
                source_fund_id: e.source_fund_id().as_str().to_owned(),
                destination_fund_id: e.destination_fund_id().as_str().to_owned(),
                transfer_id: e.transfer_id().as_str().to_owned(),
                currency: e.currency().code().to_owned(),
                amount_minor: e.amount_minor().to_string(),
                effective_at_unix_ms: e.effective_at_unix_ms().to_string(),
                observed_at_unix_ms: e.observed_at_unix_ms().to_string(),
            },
        }
    }
    fn checked(self) -> Result<FundMovementFact, CodecError> {
        Ok(match self {
            Self::Allocation {
                source_event_id,
                organization_scope,
                fund_id,
                allocation_id,
                source_cash,
                currency,
                amount_minor,
                effective_at_unix_ms,
                observed_at_unix_ms,
            } => {
                let amount = parse_i128_exact(&amount_minor)?;
                if amount <= 0 {
                    return Err(CodecError::InvalidDomain(
                        "allocation amount nonpositive".into(),
                    ));
                }
                FundMovementFact::Allocation {
                    event: FundAllocationEvent::new(
                        FundAllocationEventId::new(source_event_id).map_err(invalid)?,
                        LedgerScopeId::new(organization_scope).map_err(invalid)?,
                        FundId::new(fund_id).map_err(invalid)?,
                        FundAllocationId::new(allocation_id).map_err(invalid)?,
                        Currency::new(&currency).map_err(invalid)?,
                        amount,
                        parse_i64_exact(&effective_at_unix_ms)?,
                        parse_i64_exact(&observed_at_unix_ms)?,
                    ),
                    source_cash: AccountId::new(source_cash).map_err(invalid)?,
                }
            }
            Self::Transfer {
                source_event_id,
                organization_scope,
                source_fund_id,
                destination_fund_id,
                transfer_id,
                currency,
                amount_minor,
                effective_at_unix_ms,
                observed_at_unix_ms,
            } => {
                let amount = parse_i128_exact(&amount_minor)?;
                if amount <= 0 {
                    return Err(CodecError::InvalidDomain(
                        "transfer amount nonpositive".into(),
                    ));
                }
                FundMovementFact::Transfer(FundTransferEvent::new(
                    FundTransferEventId::new(source_event_id).map_err(invalid)?,
                    LedgerScopeId::new(organization_scope).map_err(invalid)?,
                    FundId::new(source_fund_id).map_err(invalid)?,
                    FundId::new(destination_fund_id).map_err(invalid)?,
                    FundTransferId::new(transfer_id).map_err(invalid)?,
                    Currency::new(&currency).map_err(invalid)?,
                    amount,
                    parse_i64_exact(&effective_at_unix_ms)?,
                    parse_i64_exact(&observed_at_unix_ms)?,
                ))
            }
        })
    }
}
impl FundMovementFact {
    fn key(&self) -> (&'static str, &str) {
        match self {
            Self::Allocation { event, .. } => ("allocation", event.allocation_id().as_str()),
            Self::Transfer(event) => ("transfer", event.transfer_id().as_str()),
        }
    }
}
pub fn encode_fund_movement(f: &FundMovementFact) -> Result<Vec<u8>, CodecError> {
    let bytes = serde_json::to_vec(&Envelope {
        schema_version: VERSION,
        record_type: KIND.to_owned(),
        payload: Record::from_fact(f),
    })
    .map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    if bytes.len() > MAX_BYTES {
        return Err(CodecError::InvalidPayload(
            "fund movement exceeds 1MiB".into(),
        ));
    }
    Ok(bytes)
}
pub fn decode_fund_movement(bytes: &[u8]) -> Result<FundMovementFact, CodecError> {
    if bytes.len() > MAX_BYTES {
        return Err(CodecError::InvalidPayload(
            "fund movement exceeds 1MiB".into(),
        ));
    }
    let env: Envelope<Record> =
        serde_json::from_slice(bytes).map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    if env.schema_version != VERSION {
        return Err(CodecError::UnsupportedVersion(env.schema_version));
    }
    if env.record_type != KIND {
        return Err(CodecError::UnsupportedRecordKind(env.record_type));
    }
    env.payload.checked()
}
/// Original bridge re-evaluation on a private copy of recovered Ledger.
/// Require exact already-accepted original journal; new effects are refused.
pub fn verify_fund_movement_journal(
    fact: &FundMovementFact,
    registry: &CommunityRegistry,
    ledger: &Ledger,
) -> Result<JournalEntryId, CodecError> {
    let mut shadow = ledger.clone();
    match fact {
        FundMovementFact::Allocation { event, source_cash } => {
            match FundAllocationBridge::new()
                .apply(
                    registry,
                    event,
                    &FundAllocationAccounts::new(source_cash.clone()),
                    &mut shadow,
                )
                .map_err(|e| CodecError::Replay(e.to_string()))?
            {
                FundAllocationOutcome::Replayed { journal_entry_id } => Ok(journal_entry_id),
                FundAllocationOutcome::Committed { .. } => Err(CodecError::Replay(
                    "allocation missing from original Ledger".into(),
                )),
            }
        }
        FundMovementFact::Transfer(event) => {
            match FundTransferBridge::new()
                .apply(registry, event, &mut shadow)
                .map_err(|e| CodecError::Replay(e.to_string()))?
            {
                FundTransferOutcome::Replayed { journal_entry_id } => Ok(journal_entry_id),
                FundTransferOutcome::Committed { .. } => Err(CodecError::Replay(
                    "transfer missing from original Ledger".into(),
                )),
            }
        }
    }
}
/// Typed duplicate IDs must match exact original content. This is not a proof
/// that the externally supplied stream includes all historical accepted facts.
pub fn verify_fund_movement_history<'a>(
    facts: impl IntoIterator<Item = &'a [u8]>,
    registry: &CommunityRegistry,
    ledger: &Ledger,
) -> Result<usize, CodecError> {
    let mut accepted: BTreeMap<(String, String), Vec<u8>> = BTreeMap::new();
    for bytes in facts {
        let fact = decode_fund_movement(bytes)?;
        let (kind, id) = fact.key();
        let key = (kind.to_owned(), id.to_owned());
        let canonical = encode_fund_movement(&fact)?;
        if let Some(previous) = accepted.get(&key) {
            if previous != &canonical {
                return Err(CodecError::Replay(
                    "movement event identity reused with changed facts".into(),
                ));
            }
            continue;
        }
        verify_fund_movement_journal(&fact, registry, ledger)?;
        accepted.insert(key, canonical);
    }
    Ok(accepted.len())
}
