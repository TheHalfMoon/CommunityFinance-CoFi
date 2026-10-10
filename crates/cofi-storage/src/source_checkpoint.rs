//! Bounded *unauthenticated* source-range consistency inspection.
//!
//! A source-provided hash and a coherent original-domain fact sequence are NOT
//! evidence of source custody, full tenant coverage, or provider settlement.
//! This module deliberately cannot issue a TrustedSourceCheckpoint.
//! An independent approved source root/key and the remaining registry coverage
//! must be implemented and qualified separately before any production admission.

use std::collections::BTreeSet;

use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::CodecError;
use crate::disbursement_creation::decode_disbursement_creation;
use crate::disbursement_lifecycle::{decode_disbursement_submission, decode_disbursement_terminal};
use crate::governance_approval::decode_governance_approval;
use crate::governance_fund_spend::decode_governance_fund_spend;
use crate::governance_policy::decode_governance_policy;
use crate::governance_proposal::decode_governance_proposal;
use crate::source_checkpoint_disbursement::GenesisDisbursementLineage;

const MAX_RECORDS: usize = 4096;
const MAX_RECORD_BYTES: usize = 1024 * 1024;
const MAX_TOTAL_BYTES: usize = 16 * 1024 * 1024;
const MAX_CHECKPOINT_BYTES: usize = 4096;
const DOMAIN: &[u8] = b"CoFi unauthenticated source consistency v1\0";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupportedSourceKind {
    Policy,
    Proposal,
    Approval,
    FundSpend,
    Creation,
    Submission,
    Terminal,
}
impl SupportedSourceKind {
    const fn record_type(self) -> &'static str {
        match self {
            Self::Policy => "governance.policy",
            Self::Proposal => "governance.proposal",
            Self::Approval => "governance.approval",
            Self::FundSpend => "governance.fund_spend",
            Self::Creation => "disbursement.creation",
            Self::Submission => "disbursement.submission",
            Self::Terminal => "disbursement.terminal",
        }
    }

    fn check_original_identity(
        self,
        key: &str,
        organization_id: &str,
        bytes: &[u8],
    ) -> Result<(), CodecError> {
        let expected = match self {
            Self::Policy => {
                let policy = decode_governance_policy(bytes)?;
                // The policy has no original accepted *event* identity. This
                // derived policy ID/version key is not external source proof.
                if policy.organization_id().as_str() != organization_id {
                    return Err(CodecError::Replay(
                        "original policy organization differs from declared scope".into(),
                    ));
                }
                format!("policy:{}:{}", policy.id().as_str(), policy.version())
            }
            Self::Proposal => {
                let proposal = decode_governance_proposal(bytes)?;
                if proposal.organization_id().as_str() != organization_id {
                    return Err(CodecError::Replay(
                        "original proposal organization differs from declared scope".into(),
                    ));
                }
                proposal.source_event_id().as_str().to_owned()
            }
            Self::Approval => decode_governance_approval(bytes)?
                .source_event_id()
                .as_str()
                .to_owned(),
            Self::FundSpend => {
                let spend = decode_governance_fund_spend(bytes)?;
                if spend.organization_id().as_str() != organization_id {
                    return Err(CodecError::Replay(
                        "original fund spend organization differs from declared scope".into(),
                    ));
                }
                spend.source_event_id().as_str().to_owned()
            }
            Self::Creation => decode_disbursement_creation(bytes)?
                .source_event_id()
                .as_str()
                .to_owned(),
            Self::Submission => decode_disbursement_submission(bytes)?
                .source_event_id()
                .as_str()
                .to_owned(),
            Self::Terminal => decode_disbursement_terminal(bytes)?
                .source_event_id()
                .as_str()
                .to_owned(),
        };
        if key != expected {
            return Err(CodecError::Replay(
                "source checkpoint key does not match checked original domain fact".into(),
            ));
        }
        Ok(())
    }
}

/// Entirely caller-provided scope, NOT a proof of authority.
#[derive(Debug, Clone, Copy)]
pub struct DeclaredSourceScope<'a> {
    pub authority_id: &'a str,
    pub organization_id: &'a str,
    pub environment_id: &'a str,
    pub previous_digest_hex: Option<&'a str>,
}

/// A declared original fact, with the checked original codec and source key.
#[derive(Debug, Clone, Copy)]
pub struct DeclaredSourceFact<'a> {
    pub sequence: &'a str,
    pub authority_id: &'a str,
    pub organization_id: &'a str,
    pub environment_id: &'a str,
    pub source_record_key: &'a str,
    pub kind: SupportedSourceKind,
    pub bytes: &'a [u8],
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceCheckpointDeclaration {
    schema_version: u64,
    record_type: String,
    authority_id: String,
    organization_id: String,
    environment_id: String,
    first_sequence: String,
    last_sequence: String,
    accepted_event_count: String,
    previous_digest_hex: Option<String>,
    ordered_digest_hex: String,
}

/// A checked *consistency-only* range. It does not carry any admission authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnauthenticatedRange {
    first_sequence: u64,
    last_sequence: u64,
    count: u64,
    ordered_digest_hex: String,
}
impl UnauthenticatedRange {
    #[must_use]
    pub const fn count(&self) -> u64 {
        self.count
    }

    #[must_use]
    pub const fn first_sequence(&self) -> u64 {
        self.first_sequence
    }

    #[must_use]
    pub const fn last_sequence(&self) -> u64 {
        self.last_sequence
    }

    #[must_use]
    pub fn ordered_digest_hex(&self) -> &str {
        &self.ordered_digest_hex
    }

    /// No signer/witness verification exists in this bounded G001 module.
    /// Neither caller self-attestation nor hash parity may pass this gate.
    pub fn require_independent_source_authentication(&self) -> Result<(), CodecError> {
        Err(CodecError::Replay(
            "UNAUTHENTICATED: independently pinned source root or signature unavailable".into(),
        ))
    }
}

fn invalid(reason: &str) -> CodecError {
    CodecError::InvalidPayload(reason.to_owned())
}
fn source_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-.:".contains(&b))
}
fn digest_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn sequence(value: &str) -> Result<u64, CodecError> {
    if value.is_empty()
        || value.len() > 20
        || value.len() > 1 && value.starts_with('0')
        || !value.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(invalid("noncanonical source sequence"));
    }
    value
        .parse::<u64>()
        .map_err(|_| invalid("source sequence overflow"))
}
fn frame(hash: &mut Sha256, bytes: &[u8]) {
    hash.update((bytes.len() as u64).to_be_bytes());
    hash.update(bytes);
}
fn update_scope(hash: &mut Sha256, scope: DeclaredSourceScope<'_>) {
    frame(hash, scope.authority_id.as_bytes());
    frame(hash, scope.organization_id.as_bytes());
    frame(hash, scope.environment_id.as_bytes());
    frame(hash, scope.previous_digest_hex.unwrap_or("").as_bytes());
}

/// Compute only an *untrusted* digest of the entire declared, checked range.
/// Callers can recompute this digest for a fabricated dataset. Never interpret
/// a match to a caller's checkpoint as proof of an independent accepted stream.
pub fn compute_untrusted_range(
    scope: DeclaredSourceScope<'_>,
    facts: &[DeclaredSourceFact<'_>],
) -> Result<UnauthenticatedRange, CodecError> {
    if !source_id(scope.authority_id)
        || !source_id(scope.organization_id)
        || !source_id(scope.environment_id)
        || facts.is_empty()
        || facts.len() > MAX_RECORDS
    {
        return Err(invalid("invalid source scope or range size"));
    }
    if scope.previous_digest_hex.is_some_and(|v| !digest_hex(v)) {
        return Err(invalid("invalid prior checkpoint digest"));
    }
    let first = sequence(facts[0].sequence)?;
    if first == 0
        || first == 1 && scope.previous_digest_hex.is_some()
        || first > 1 && scope.previous_digest_hex.is_none()
    {
        return Err(invalid(
            "source range missing valid predecessor declaration",
        ));
    }
    let mut seen = BTreeSet::new();
    // Only complete caller-declared genesis ranges can supply prior in-range
    // lifecycle facts. This does not prove independent source custody.
    let mut genesis_disbursements = GenesisDisbursementLineage::default();
    let mut hash = Sha256::new();
    hash.update(DOMAIN);
    update_scope(&mut hash, scope);
    frame(&mut hash, facts[0].sequence.as_bytes());
    let mut total = 0usize;
    for (index, fact) in facts.iter().enumerate() {
        let expected = first
            .checked_add(index as u64)
            .ok_or_else(|| invalid("source range exceeds u64"))?;
        if sequence(fact.sequence)? != expected {
            return Err(invalid("source range contains gap, duplicate, or reorder"));
        }
        if fact.authority_id != scope.authority_id
            || fact.organization_id != scope.organization_id
            || fact.environment_id != scope.environment_id
        {
            return Err(invalid(
                "cross-authority, organization or environment source",
            ));
        }
        if !source_id(fact.source_record_key) || !seen.insert(fact.source_record_key) {
            return Err(invalid("source key empty, invalid or duplicated"));
        }
        if fact.bytes.is_empty() || fact.bytes.len() > MAX_RECORD_BYTES {
            return Err(invalid("source fact exceeds bounded length"));
        }
        total = total
            .checked_add(fact.bytes.len())
            .ok_or_else(|| invalid("source range size overflow"))?;
        if total > MAX_TOTAL_BYTES {
            return Err(invalid("source range exceeds total byte bound"));
        }
        fact.kind.check_original_identity(
            fact.source_record_key,
            scope.organization_id,
            fact.bytes,
        )?;
        if first == 1 {
            genesis_disbursements.check(fact.kind, fact.bytes)?;
        }
        frame(&mut hash, fact.sequence.as_bytes());
        frame(&mut hash, fact.kind.record_type().as_bytes());
        frame(&mut hash, fact.source_record_key.as_bytes());
        frame(&mut hash, fact.bytes);
    }
    let last = first
        .checked_add((facts.len() - 1) as u64)
        .ok_or_else(|| invalid("source range exceeds u64"))?;
    frame(&mut hash, last.to_string().as_bytes());
    let digest = hash.finalize();
    let hex = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    Ok(UnauthenticatedRange {
        first_sequence: first,
        last_sequence: last,
        count: facts.len() as u64,
        ordered_digest_hex: hex,
    })
}

/// Strict declaration comparison. This returns a consistency-only result and
/// never verifies an external signature, trusted checkpoint or tenant cutoff.
pub fn inspect_untrusted_source_checkpoint(
    declaration_bytes: &[u8],
    facts: &[DeclaredSourceFact<'_>],
) -> Result<UnauthenticatedRange, CodecError> {
    if declaration_bytes.len() > MAX_CHECKPOINT_BYTES {
        return Err(invalid("checkpoint declaration exceeds size bound"));
    }
    let cp: SourceCheckpointDeclaration = serde_json::from_slice(declaration_bytes)
        .map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    if cp.schema_version != 1 {
        return Err(CodecError::UnsupportedVersion(cp.schema_version));
    }
    if cp.record_type != "source.checkpoint.declaration" {
        return Err(CodecError::UnsupportedRecordKind(cp.record_type));
    }
    if !digest_hex(&cp.ordered_digest_hex) {
        return Err(invalid("invalid declared digest"));
    }
    let scope = DeclaredSourceScope {
        authority_id: &cp.authority_id,
        organization_id: &cp.organization_id,
        environment_id: &cp.environment_id,
        previous_digest_hex: cp.previous_digest_hex.as_deref(),
    };
    let computed = compute_untrusted_range(scope, facts)?;
    if computed.first_sequence != sequence(&cp.first_sequence)?
        || computed.last_sequence != sequence(&cp.last_sequence)?
        || computed.count != sequence(&cp.accepted_event_count)?
        || computed.ordered_digest_hex != cp.ordered_digest_hex
    {
        return Err(CodecError::Replay(
            "checkpoint does not match supplied accepted source range".into(),
        ));
    }
    Ok(computed)
}
