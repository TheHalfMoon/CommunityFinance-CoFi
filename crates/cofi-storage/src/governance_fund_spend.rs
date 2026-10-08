//! Checked accepted spending source/journal parity after original governance.
//! ApprovedSpendingAuthorization is built ONLY by GovernanceEngine quorum.
//! FundSpendBridge::verify_committed is read-only; no money can be posted
//! by this module. Caller-supplied historic event stream remains untrusted.

use crate::governance_approval::replay_governance_approvals;
use crate::{CodecError, parse_i64_exact, parse_i128_exact};
use cofi_community::{CommunityId, CommunityRegistry, FundId, OrganizationId};
use cofi_governance::SpendingProposalId;
use cofi_ledger::{AccountId, Currency, Ledger};
use cofi_spending::{
    ApprovedFundSpendEvent, FundSpendBridge, FundSpendEventId, FundSpendId, VerifiedFundSpend,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const VERSION: u64 = 1;
const KIND: &str = "governance.fund_spend";
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
struct SpendRecord {
    source_event_id: String,
    spend_id: String,
    proposal_id: String,
    organization_id: String,
    community_id: String,
    fund_id: String,
    currency: String,
    amount_minor: String,
    purpose_reference: String,
    expense_account_id: String,
    executed_at_unix_ms: String,
    observed_at_unix_ms: String,
}
fn invalid<E: std::fmt::Display>(e: E) -> CodecError {
    CodecError::InvalidDomain(e.to_string())
}
impl SpendRecord {
    fn from_event(e: &ApprovedFundSpendEvent) -> Self {
        Self {
            source_event_id: e.source_event_id().as_str().to_owned(),
            spend_id: e.spend_id().as_str().to_owned(),
            proposal_id: e.proposal_id().as_str().to_owned(),
            organization_id: e.organization_id().as_str().to_owned(),
            community_id: e.community_id().as_str().to_owned(),
            fund_id: e.fund_id().as_str().to_owned(),
            currency: e.currency().code().to_owned(),
            amount_minor: e.amount_minor().to_string(),
            purpose_reference: e.purpose_reference().to_owned(),
            expense_account_id: e.expense_account_id().as_str().to_owned(),
            executed_at_unix_ms: e.executed_at_unix_ms().to_string(),
            observed_at_unix_ms: e.observed_at_unix_ms().to_string(),
        }
    }
    fn checked(self) -> Result<ApprovedFundSpendEvent, CodecError> {
        ApprovedFundSpendEvent::new(
            FundSpendEventId::new(self.source_event_id).map_err(invalid)?,
            FundSpendId::new(self.spend_id).map_err(invalid)?,
            SpendingProposalId::new(self.proposal_id).map_err(invalid)?,
            OrganizationId::new(self.organization_id).map_err(invalid)?,
            CommunityId::new(self.community_id).map_err(invalid)?,
            FundId::new(self.fund_id).map_err(invalid)?,
            Currency::new(&self.currency).map_err(invalid)?,
            parse_i128_exact(&self.amount_minor)?,
            self.purpose_reference,
            AccountId::new(self.expense_account_id).map_err(invalid)?,
            parse_i64_exact(&self.executed_at_unix_ms)?,
            parse_i64_exact(&self.observed_at_unix_ms)?,
        )
        .map_err(invalid)
    }
}
pub fn encode_governance_fund_spend(e: &ApprovedFundSpendEvent) -> Result<Vec<u8>, CodecError> {
    let bytes = serde_json::to_vec(&Envelope {
        schema_version: VERSION,
        record_type: KIND.to_owned(),
        payload: SpendRecord::from_event(e),
    })
    .map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    if bytes.len() > MAX_BYTES {
        return Err(CodecError::InvalidPayload(
            "governance fund spend exceeds 1MiB".into(),
        ));
    }
    if decode_governance_fund_spend(&bytes)? != *e {
        return Err(CodecError::Replay(
            "fund spending source roundtrip differs".into(),
        ));
    }
    Ok(bytes)
}
pub fn decode_governance_fund_spend(bytes: &[u8]) -> Result<ApprovedFundSpendEvent, CodecError> {
    if bytes.len() > MAX_BYTES {
        return Err(CodecError::InvalidPayload(
            "governance fund spend exceeds 1MiB".into(),
        ));
    }
    let e: Envelope<SpendRecord> =
        serde_json::from_slice(bytes).map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    if e.schema_version != VERSION {
        return Err(CodecError::UnsupportedVersion(e.schema_version));
    }
    if e.record_type != KIND {
        return Err(CodecError::UnsupportedRecordKind(e.record_type));
    }
    e.payload.checked()
}
/// Verify exact original approved immutable authorization and journal.
/// Caller must separately establish ledger/source authenticity and cutoff.
pub fn verify_governance_fund_spend(
    spend_receipt: &[u8],
    governance: &cofi_governance::GovernanceEngine,
    community: &CommunityRegistry,
    ledger: &Ledger,
) -> Result<VerifiedFundSpend, CodecError> {
    let e = decode_governance_fund_spend(spend_receipt)?;
    FundSpendBridge::new()
        .verify_committed(community, governance, &e, ledger)
        .map_err(|e| CodecError::Replay(e.to_string()))
}
/// Reconstruct policies, accepted proposals, ordered original approvals
/// and quorum *before* checking each preexisting committed journal.
/// Duplicate source, spend or proposal consumption cannot be changed.
pub fn verify_governance_fund_spend_history<'a, 'b, 'c, 'd>(
    policies: impl IntoIterator<Item = &'a [u8]>,
    proposals: impl IntoIterator<Item = &'b [u8]>,
    approvals: impl IntoIterator<Item = &'c [u8]>,
    spends: impl IntoIterator<Item = &'d [u8]>,
    community: &CommunityRegistry,
    ledger: &Ledger,
) -> Result<usize, CodecError> {
    let gov = replay_governance_approvals(policies, proposals, approvals, community)?;
    let mut sources: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut spend_ids: BTreeMap<String, String> = BTreeMap::new();
    let mut consumed: BTreeMap<String, String> = BTreeMap::new();
    for bytes in spends {
        let event = decode_governance_fund_spend(bytes)?;
        let source = event.source_event_id().as_str().to_owned();
        let spend = event.spend_id().as_str().to_owned();
        let proposal = event.proposal_id().as_str().to_owned();
        let canonical = encode_governance_fund_spend(&event)?;
        if let Some(original) = sources.get(&source) {
            if original != &canonical {
                return Err(CodecError::Replay(
                    "changed accepted fund spend event identity".into(),
                ));
            }
            continue;
        }
        if spend_ids.contains_key(&spend) {
            return Err(CodecError::Replay(
                "fund spend business identity consumed twice".into(),
            ));
        }
        if consumed.contains_key(&proposal) {
            return Err(CodecError::Replay("governance proposal spent twice".into()));
        }
        verify_governance_fund_spend(bytes, &gov, community, ledger)?;
        sources.insert(source.clone(), canonical);
        spend_ids.insert(spend, source);
        consumed.insert(proposal, event.spend_id().as_str().to_owned());
    }
    Ok(sources.len())
}
