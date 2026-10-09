//! Bounded caller-ordered P21 → P22 → P23 accepted billing ancestry.
//!
//! Embedded valid upstream receipts alone cannot prove an earlier accepted
//! historical fact. Check every ancestor against a previously encountered
//! canonical source value, then rebuild state using original registries.
//! No source authentication, accepted tenant cutoff, ledger posting, invoice
//! issuance or real financial effect is performed.

use std::collections::{BTreeMap, BTreeSet};

use cofi_finalization_authorization::AuthorizedFinalizationRegistry;
use cofi_invoice_authorization::AuthorizedDraftRegistry;
use cofi_rating_authorization::AuthorizedRatingRegistry;
use serde_json::Value;

use crate::CodecError;
use crate::authorized_draft::{decode_authorized_draft, replay_authorized_drafts};
use crate::authorized_finalization::{
    decode_authorized_finalization, replay_authorized_finalizations,
};
use crate::authorized_rating::{decode_authorized_rating, replay_authorized_ratings};

const MAX_FACT_BYTES: usize = 1024 * 1024;
const MAX_FACTS: usize = 4096;
const MAX_TOTAL_BYTES: usize = 16 * 1024 * 1024;

/// One caller-asserted accepted fact in historical inter-kind order.
#[derive(Debug, Clone, Copy)]
pub enum AcceptedBillingFact<'a> {
    Rating(&'a [u8]),
    Draft(&'a [u8]),
    Finalization(&'a [u8]),
}

/// Original registry projections from one internally consistent source stream.
pub struct VerifiedBillingHistory {
    ratings: AuthorizedRatingRegistry,
    drafts: AuthorizedDraftRegistry,
    finalizations: AuthorizedFinalizationRegistry,
}
impl VerifiedBillingHistory {
    #[must_use]
    pub const fn ratings(&self) -> &AuthorizedRatingRegistry {
        &self.ratings
    }
    #[must_use]
    pub const fn drafts(&self) -> &AuthorizedDraftRegistry {
        &self.drafts
    }
    #[must_use]
    pub const fn finalizations(&self) -> &AuthorizedFinalizationRegistry {
        &self.finalizations
    }
}

fn invalid(reason: &str) -> CodecError {
    CodecError::InvalidPayload(reason.to_owned())
}

fn key<'a>(source: &'a Value, pointer: &str) -> Result<&'a str, CodecError> {
    source
        .pointer(pointer)
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| invalid("missing accepted billing source event identity"))
}

fn source_value(bytes: &[u8], total: &mut usize) -> Result<Value, CodecError> {
    if bytes.is_empty() || bytes.len() > MAX_FACT_BYTES {
        return Err(invalid(
            "accepted billing source fact exceeds intake bounds",
        ));
    }
    *total = total
        .checked_add(bytes.len())
        .ok_or_else(|| invalid("accepted billing stream byte length overflow"))?;
    if *total > MAX_TOTAL_BYTES {
        return Err(invalid(
            "accepted billing stream exceeds total intake bound",
        ));
    }
    serde_json::from_slice(bytes).map_err(|e| CodecError::InvalidPayload(e.to_string()))
}

/// Cross-check accepted P21, P22 and P23 ancestry in the supplied original
/// fact order. Unlike idempotent normal domain commands, an *accepted source
/// fact* cannot appear twice. Original domain registries are the only state
/// builders; an embedded P21/P22 receipt cannot manufacture an earlier fact.
///
/// The input itself is not authenticated. Coherent omission or fabrication
/// outside this supplied range is NOT detected. External witness/source cutoff
/// and atomic G002 persistence remain separate production hard gates.
pub fn replay_ordered_authorized_billing_history<'a>(
    facts: impl IntoIterator<Item = AcceptedBillingFact<'a>>,
) -> Result<VerifiedBillingHistory, CodecError> {
    let mut rating_sources = BTreeMap::<String, Value>::new();
    let mut draft_sources = BTreeMap::<String, Value>::new();
    let mut finalization_ids = BTreeSet::<String>::new();
    let mut consumed_rating_ids = BTreeSet::<String>::new();
    let mut consumed_draft_ids = BTreeSet::<String>::new();

    let mut p21 = Vec::<&'a [u8]>::new();
    let mut p22 = Vec::<&'a [u8]>::new();
    let mut p23 = Vec::<&'a [u8]>::new();
    let mut count = 0usize;
    let mut total = 0usize;

    for fact in facts {
        count = count
            .checked_add(1)
            .ok_or_else(|| invalid("accepted billing source count overflow"))?;
        if count > MAX_FACTS {
            return Err(invalid("too many accepted billing source facts"));
        }
        match fact {
            AcceptedBillingFact::Rating(bytes) => {
                let value = source_value(bytes, &mut total)?;
                let id = key(&value, "/payload/expected_rating_event_id")?.to_owned();
                if rating_sources.contains_key(&id) {
                    return Err(CodecError::Replay(
                        "duplicate accepted P21 rating source fact".to_owned(),
                    ));
                }
                decode_authorized_rating(bytes)?;
                rating_sources.insert(id, value);
                p21.push(bytes);
            }
            AcceptedBillingFact::Draft(bytes) => {
                let value = source_value(bytes, &mut total)?;
                let id = key(&value, "/payload/event_id")?.to_owned();
                if draft_sources.contains_key(&id) {
                    return Err(CodecError::Replay(
                        "duplicate accepted P22 draft source fact".to_owned(),
                    ));
                }
                let embedded = value
                    .pointer("/payload/authorizations")
                    .and_then(Value::as_array)
                    .ok_or_else(|| invalid("missing accepted P22 P21 ancestry"))?;
                if embedded.is_empty() {
                    return Err(invalid("P22 source has no preceding accepted P21 facts"));
                }
                let mut draft_rating_ids = BTreeSet::<String>::new();
                for receipt in embedded {
                    let ancestor_id = key(receipt, "/payload/expected_rating_event_id")?.to_owned();
                    if rating_sources.get(&ancestor_id) != Some(receipt)
                        || consumed_rating_ids.contains(&ancestor_id)
                        || !draft_rating_ids.insert(ancestor_id)
                    {
                        return Err(CodecError::Replay(
                            "P22 includes absent, altered, late or consumed accepted P21 source"
                                .to_owned(),
                        ));
                    }
                }
                decode_authorized_draft(bytes)?;
                consumed_rating_ids.extend(draft_rating_ids);
                draft_sources.insert(id, value);
                p22.push(bytes);
            }
            AcceptedBillingFact::Finalization(bytes) => {
                let value = source_value(bytes, &mut total)?;
                let id = key(&value, "/payload/source_event_id")?.to_owned();
                if finalization_ids.contains(&id) {
                    return Err(CodecError::Replay(
                        "duplicate accepted P23 finalization source fact".to_owned(),
                    ));
                }
                let ancestor = value
                    .pointer("/payload/authorized_draft")
                    .ok_or_else(|| invalid("missing accepted P23 P22 ancestry"))?;
                let draft_id = key(ancestor, "/payload/event_id")?.to_owned();
                if draft_sources.get(&draft_id) != Some(ancestor)
                    || consumed_draft_ids.contains(&draft_id)
                {
                    return Err(CodecError::Replay(
                        "P23 includes absent, altered, late or consumed accepted P22 source"
                            .to_owned(),
                    ));
                }
                decode_authorized_finalization(bytes)?;
                consumed_draft_ids.insert(draft_id);
                finalization_ids.insert(id);
                p23.push(bytes);
            }
        }
    }

    // Only original domain registries rebuild accepted state and consumption
    // indexes. These replay APIs verify original checked source ancestry too.
    let ratings = replay_authorized_ratings(p21)?;
    let drafts = replay_authorized_drafts(p22)?;
    let finalizations = replay_authorized_finalizations(p23)?;
    Ok(VerifiedBillingHistory {
        ratings,
        drafts,
        finalizations,
    })
}
