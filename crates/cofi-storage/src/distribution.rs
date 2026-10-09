//! Checked immutable distribution source and original journal parity.
//!
//! Never imports a private financial registry or executes on the caller's
//! Ledger. This validates a *supplied* source against a *supplied* Ledger;
//! neither is by itself independently authenticated or complete.

use std::collections::BTreeMap;

use cofi_community::{
    CommunityRegistry, FundId, RevenueDistributionBridge, RevenueDistributionEvent,
    RevenueDistributionEventId, RevenueDistributionId, RevenueDistributionOutcome,
    RevenueSplitRule, RevenueSplitRuleId,
};
use cofi_ledger::{Currency, JournalEntryId, Ledger, LedgerScopeId};
use serde::{Deserialize, Serialize};

use crate::revenue_split_rule::{decode_revenue_split_rule, encode_revenue_split_rule};
use crate::{CodecError, parse_i64_exact, parse_i128_exact};

const VERSION: u64 = 1;
const KIND: &str = "community.distribution";
const MAX_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistributionFact {
    event: RevenueDistributionEvent,
    rule: RevenueSplitRule,
}
impl DistributionFact {
    #[must_use]
    pub const fn new(event: RevenueDistributionEvent, rule: RevenueSplitRule) -> Self {
        Self { event, rule }
    }
    #[must_use]
    pub const fn event(&self) -> &RevenueDistributionEvent {
        &self.event
    }
    #[must_use]
    pub const fn rule(&self) -> &RevenueSplitRule {
        &self.rule
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
struct EventRecord {
    source_event_id: String,
    organization_scope: String,
    source_fund_id: String,
    distribution_id: String,
    rule_id: String,
    rule_version: u32,
    currency: String,
    amount_minor: String,
    effective_at_unix_ms: String,
    observed_at_unix_ms: String,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleLeg {
    destination_fund_id: String,
    basis_points: u16,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RulePayload {
    id: String,
    version: u32,
    legs: Vec<RuleLeg>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleEnvelope {
    schema_version: u64,
    record_type: String,
    payload: RulePayload,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DistributionRecord {
    event: EventRecord,
    rule: RuleEnvelope,
}
fn invalid<E: std::fmt::Display>(e: E) -> CodecError {
    CodecError::InvalidDomain(e.to_string())
}
fn record_from_fact(fact: &DistributionFact) -> Result<DistributionRecord, CodecError> {
    let e = fact.event();
    let rule_bytes = encode_revenue_split_rule(fact.rule())?;
    let rule = serde_json::from_slice(&rule_bytes)
        .map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    Ok(DistributionRecord {
        event: EventRecord {
            source_event_id: e.source_event_id().as_str().to_owned(),
            organization_scope: e.organization_scope().as_str().to_owned(),
            source_fund_id: e.source_fund_id().as_str().to_owned(),
            distribution_id: e.distribution_id().as_str().to_owned(),
            rule_id: e.rule_id().as_str().to_owned(),
            rule_version: e.rule_version(),
            currency: e.currency().code().to_owned(),
            amount_minor: e.amount_minor().to_string(),
            effective_at_unix_ms: e.effective_at_unix_ms().to_string(),
            observed_at_unix_ms: e.observed_at_unix_ms().to_string(),
        },
        rule,
    })
}
pub fn encode_distribution(fact: &DistributionFact) -> Result<Vec<u8>, CodecError> {
    let bytes = serde_json::to_vec(&Envelope {
        schema_version: VERSION,
        record_type: KIND.to_owned(),
        payload: record_from_fact(fact)?,
    })
    .map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    if bytes.len() > MAX_BYTES {
        return Err(CodecError::InvalidPayload(
            "distribution record exceeds 1MiB".to_owned(),
        ));
    }
    // Reject unchecked constructor combinations, not only on decode.
    decode_distribution(&bytes)?;
    Ok(bytes)
}
pub fn decode_distribution(bytes: &[u8]) -> Result<DistributionFact, CodecError> {
    if bytes.len() > MAX_BYTES {
        return Err(CodecError::InvalidPayload(
            "distribution record exceeds 1MiB".to_owned(),
        ));
    }
    let envelope: Envelope<DistributionRecord> =
        serde_json::from_slice(bytes).map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    if envelope.schema_version != VERSION {
        return Err(CodecError::UnsupportedVersion(envelope.schema_version));
    }
    if envelope.record_type != KIND {
        return Err(CodecError::UnsupportedRecordKind(envelope.record_type));
    }
    let DistributionRecord { event: e, rule } = envelope.payload;
    // The existing canonical revenue-split rule codec validates all version,
    // identity, sorted unique destination and exact basis-point constraints.
    let rule_json =
        serde_json::to_vec(&rule).map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    let rule = decode_revenue_split_rule(&rule_json)?;
    if rule.id().as_str() != e.rule_id || rule.version() != e.rule_version {
        return Err(CodecError::InvalidDomain(
            "distribution rule identity/version mismatch".into(),
        ));
    }
    let amount = parse_i128_exact(&e.amount_minor)?;
    if amount <= 0 {
        return Err(CodecError::InvalidDomain(
            "distribution amount must be positive".into(),
        ));
    }
    let event = RevenueDistributionEvent::new(
        RevenueDistributionEventId::new(e.source_event_id).map_err(invalid)?,
        LedgerScopeId::new(e.organization_scope).map_err(invalid)?,
        FundId::new(e.source_fund_id).map_err(invalid)?,
        RevenueDistributionId::new(e.distribution_id).map_err(invalid)?,
        RevenueSplitRuleId::new(e.rule_id).map_err(invalid)?,
        e.rule_version,
        Currency::new(&e.currency).map_err(invalid)?,
        amount,
        parse_i64_exact(&e.effective_at_unix_ms)?,
        parse_i64_exact(&e.observed_at_unix_ms)?,
    );
    Ok(DistributionFact { event, rule })
}
/// Run only the original domain distribution algorithm on a PRIVATE Ledger
/// clone. Accept only an already-existing identical journal (Replayed).
pub fn verify_distribution_journal(
    fact: &DistributionFact,
    registry: &CommunityRegistry,
    ledger: &Ledger,
) -> Result<JournalEntryId, CodecError> {
    let mut scratch = ledger.clone();
    match RevenueDistributionBridge::new()
        .apply(registry, fact.event(), fact.rule(), &mut scratch)
        .map_err(|e| CodecError::Replay(e.to_string()))?
    {
        RevenueDistributionOutcome::Replayed { journal_entry_id } => Ok(journal_entry_id),
        RevenueDistributionOutcome::Committed { .. } => Err(CodecError::Replay(
            "distribution missing from accepted original Ledger".to_owned(),
        )),
    }
}
/// Reject changed same business identity and same source ID claimed by
/// different business IDs. Supplied stream completeness remains untrusted.
pub fn verify_distribution_history<'a>(
    facts: impl IntoIterator<Item = &'a [u8]>,
    registry: &CommunityRegistry,
    ledger: &Ledger,
) -> Result<usize, CodecError> {
    let mut business: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut events: BTreeMap<String, String> = BTreeMap::new();
    for bytes in facts {
        let fact = decode_distribution(bytes)?;
        let event_id = fact.event().source_event_id().as_str();
        let business_id = fact.event().distribution_id().as_str();
        let canonical = encode_distribution(&fact)?;
        if let Some(existing) = business.get(business_id) {
            if existing != &canonical {
                return Err(CodecError::Replay(
                    "distribution business identity changed".to_owned(),
                ));
            }
            continue;
        }
        if let Some(existing) = events.get(event_id) {
            if existing != business_id {
                return Err(CodecError::Replay(
                    "distribution source identity consumed twice".to_owned(),
                ));
            }
        }
        verify_distribution_journal(&fact, registry, ledger)?;
        business.insert(business_id.to_owned(), canonical);
        events.insert(event_id.to_owned(), business_id.to_owned());
    }
    Ok(business.len())
}

/// Original accepted source plus the caller's claimed after-event Ledger.
/// The checkpoint is comparison material, NOT an independent source witness.
#[derive(Debug, Clone, Copy)]
pub struct DistributionStage<'a> {
    pub source: &'a [u8],
    pub reference: &'a Ledger,
}

/// Read-only original-domain projection from a staged accepted source list.
#[derive(Debug, Clone)]
pub struct RebuiltDistributionHistory {
    ledger: Ledger,
    count: usize,
}
impl RebuiltDistributionHistory {
    #[must_use]
    pub const fn ledger(&self) -> &Ledger {
        &self.ledger
    }
    #[must_use]
    pub const fn count(&self) -> usize {
        self.count
    }
}

/// Replay original revenue distribution transitions on a private genesis
/// Ledger, comparing exact emitted journals, full account definitions and
/// balances to each caller-provided historical checkpoint.
///
/// This supports ONLY source-order distribution facts with a fixed prior
/// account inventory and a separately caller-supplied final cutoff. It is NOT
/// an authenticated source sequence, a complete financial ledger, or a live
/// execution path. No references or caller Ledger are mutated.
pub fn rebuild_staged_distribution_history(
    stages: &[DistributionStage<'_>],
    registry: &CommunityRegistry,
    genesis: &Ledger,
    final_reference: &Ledger,
) -> Result<RebuiltDistributionHistory, CodecError> {
    use std::collections::BTreeSet;

    use cofi_community::RevenueDistributionOutcome;

    use crate::account_parity::verify_all_accounts;

    if stages.is_empty() || stages.len() > 4096 {
        return Err(CodecError::Replay(
            "missing or oversized original distribution stage list".into(),
        ));
    }
    let mut ledger = genesis.clone();
    let mut event_ids = BTreeSet::<String>::new();
    let mut business_ids = BTreeSet::<String>::new();
    let mut accepted_journals = BTreeSet::<JournalEntryId>::new();
    for (index, stage) in stages.iter().enumerate() {
        let fact = decode_distribution(stage.source)?;
        if !event_ids.insert(fact.event().source_event_id().as_str().to_owned())
            || !business_ids.insert(fact.event().distribution_id().as_str().to_owned())
        {
            return Err(CodecError::Replay(
                "duplicate accepted distribution source or business identity".into(),
            ));
        }
        let outcome = RevenueDistributionBridge::new()
            .apply(registry, fact.event(), fact.rule(), &mut ledger)
            .map_err(|e| CodecError::Replay(e.to_string()))?;
        let journal = match outcome {
            RevenueDistributionOutcome::Committed { journal_entry_id } => journal_entry_id,
            RevenueDistributionOutcome::Replayed { .. } => {
                return Err(CodecError::Replay(
                    "accepted distribution must be a first-time original transition".into(),
                ));
            }
        };
        if !accepted_journals.insert(journal.clone()) {
            return Err(CodecError::Replay(
                "duplicate accepted distribution journal identity".into(),
            ));
        }
        let expected_count = genesis
            .entry_count()
            .checked_add(index + 1)
            .ok_or_else(|| CodecError::Replay("original stage count overflow".into()))?;
        if ledger.entry_count() != expected_count || stage.reference.entry_count() != expected_count
        {
            return Err(CodecError::Replay(
                "original distribution stage journal count missing or augmented".into(),
            ));
        }
        for id in &accepted_journals {
            if ledger.entry(id).is_none() || ledger.entry(id) != stage.reference.entry(id) {
                return Err(CodecError::Replay(
                    "distribution original journal contents differ at stage".into(),
                ));
            }
        }
        // A matching count, emitted distribution journal and running balances
        // do not prove that preexisting genesis journals were not substituted
        // (for example, original seed metadata altered with equal postings).
        if !ledger.entries().eq(stage.reference.entries()) {
            return Err(CodecError::Replay(
                "original journal inventory/content differs at distribution stage".into(),
            ));
        }
        verify_all_accounts(genesis, &ledger, stage.reference)?;
    }
    if ledger.entry_count() != final_reference.entry_count() {
        return Err(CodecError::Replay(
            "final distribution source count does not match declared cutoff".into(),
        ));
    }
    for id in &accepted_journals {
        if ledger.entry(id).is_none() || ledger.entry(id) != final_reference.entry(id) {
            return Err(CodecError::Replay(
                "final distribution original journal differs".into(),
            ));
        }
    }
    if !ledger.entries().eq(final_reference.entries()) {
        return Err(CodecError::Replay(
            "final original journal inventory/content differs".into(),
        ));
    }
    verify_all_accounts(genesis, &ledger, final_reference)?;
    Ok(RebuiltDistributionHistory {
        ledger,
        count: stages.len(),
    })
}
