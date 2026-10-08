//! Versioned replay of CoFi's base community registration facts.
//! Registry methods own cross-organization, parent, fund/ledger and uniqueness
//! validation; this codec is not a trusted durable source or tenant boundary.
use crate::CodecError;
use cofi_community::{
    Community, CommunityId, CommunityRegistry, Fund, FundId, Membership, MembershipId,
    MembershipRole, MembershipStatus, Organization, OrganizationId, Party, PartyId, PartyKind,
};
use cofi_ledger::{AccountId, Currency, Ledger};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const VERSION: u64 = 1;
const KIND: &str = "community.fact";
const MAX_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommunityFact {
    Organization(Organization),
    Party(Party),
    Community(Community),
    Membership(Membership),
    Fund(Fund),
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum StoredPartyKind {
    Person,
    Organization,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum StoredRole {
    Owner,
    Admin,
    Treasurer,
    Member,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum StoredStatus {
    Active,
    Suspended,
}
impl From<PartyKind> for StoredPartyKind {
    fn from(v: PartyKind) -> Self {
        match v {
            PartyKind::Person => Self::Person,
            PartyKind::Organization => Self::Organization,
        }
    }
}
impl From<StoredPartyKind> for PartyKind {
    fn from(v: StoredPartyKind) -> Self {
        match v {
            StoredPartyKind::Person => Self::Person,
            StoredPartyKind::Organization => Self::Organization,
        }
    }
}
impl From<MembershipRole> for StoredRole {
    fn from(v: MembershipRole) -> Self {
        match v {
            MembershipRole::Owner => Self::Owner,
            MembershipRole::Admin => Self::Admin,
            MembershipRole::Treasurer => Self::Treasurer,
            MembershipRole::Member => Self::Member,
        }
    }
}
impl From<StoredRole> for MembershipRole {
    fn from(v: StoredRole) -> Self {
        match v {
            StoredRole::Owner => Self::Owner,
            StoredRole::Admin => Self::Admin,
            StoredRole::Treasurer => Self::Treasurer,
            StoredRole::Member => Self::Member,
        }
    }
}
impl From<MembershipStatus> for StoredStatus {
    fn from(v: MembershipStatus) -> Self {
        match v {
            MembershipStatus::Active => Self::Active,
            MembershipStatus::Suspended => Self::Suspended,
        }
    }
}
impl From<StoredStatus> for MembershipStatus {
    fn from(v: StoredStatus) -> Self {
        match v {
            StoredStatus::Active => Self::Active,
            StoredStatus::Suspended => Self::Suspended,
        }
    }
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
enum StoredFact {
    Organization {
        id: String,
    },
    Party {
        id: String,
        organization_id: String,
        party_kind: StoredPartyKind,
    },
    Community {
        id: String,
        organization_id: String,
    },
    Membership {
        id: String,
        party_id: String,
        community_id: String,
        role: StoredRole,
        status: StoredStatus,
    },
    Fund {
        id: String,
        community_id: String,
        ledger_account_id: String,
        currency: String,
    },
}
impl StoredFact {
    fn from_domain(v: &CommunityFact) -> Self {
        match v {
            CommunityFact::Organization(v) => Self::Organization {
                id: v.id().as_str().to_owned(),
            },
            CommunityFact::Party(v) => Self::Party {
                id: v.id().as_str().to_owned(),
                organization_id: v.organization_id().as_str().to_owned(),
                party_kind: v.kind().into(),
            },
            CommunityFact::Community(v) => Self::Community {
                id: v.id().as_str().to_owned(),
                organization_id: v.organization_id().as_str().to_owned(),
            },
            CommunityFact::Membership(v) => Self::Membership {
                id: v.id().as_str().to_owned(),
                party_id: v.party_id().as_str().to_owned(),
                community_id: v.community_id().as_str().to_owned(),
                role: v.role().into(),
                status: v.status().into(),
            },
            CommunityFact::Fund(v) => Self::Fund {
                id: v.id().as_str().to_owned(),
                community_id: v.community_id().as_str().to_owned(),
                ledger_account_id: v.ledger_account_id().as_str().to_owned(),
                currency: v.currency().code().to_owned(),
            },
        }
    }
    fn checked_domain(self) -> Result<CommunityFact, CodecError> {
        let check = |e: cofi_community::CommunityError| CodecError::InvalidDomain(e.to_string());
        Ok(match self {
            Self::Organization { id } => CommunityFact::Organization(Organization::new(
                OrganizationId::new(id).map_err(check)?,
            )),
            Self::Party {
                id,
                organization_id,
                party_kind,
            } => CommunityFact::Party(Party::new(
                PartyId::new(id).map_err(check)?,
                OrganizationId::new(organization_id).map_err(check)?,
                party_kind.into(),
            )),
            Self::Community {
                id,
                organization_id,
            } => CommunityFact::Community(Community::new(
                CommunityId::new(id).map_err(check)?,
                OrganizationId::new(organization_id).map_err(check)?,
            )),
            Self::Membership {
                id,
                party_id,
                community_id,
                role,
                status,
            } => CommunityFact::Membership(Membership::new(
                MembershipId::new(id).map_err(check)?,
                PartyId::new(party_id).map_err(check)?,
                CommunityId::new(community_id).map_err(check)?,
                role.into(),
                status.into(),
            )),
            Self::Fund {
                id,
                community_id,
                ledger_account_id,
                currency,
            } => CommunityFact::Fund(Fund::new(
                FundId::new(id).map_err(check)?,
                CommunityId::new(community_id).map_err(check)?,
                AccountId::new(ledger_account_id)
                    .map_err(|e| CodecError::InvalidDomain(e.to_string()))?,
                Currency::new(&currency).map_err(|e| CodecError::InvalidDomain(e.to_string()))?,
            )),
        })
    }
}
impl CommunityFact {
    fn identity(&self) -> (&'static str, &str) {
        match self {
            Self::Organization(v) => ("organization", v.id().as_str()),
            Self::Party(v) => ("party", v.id().as_str()),
            Self::Community(v) => ("community", v.id().as_str()),
            Self::Membership(v) => ("membership", v.id().as_str()),
            Self::Fund(v) => ("fund", v.id().as_str()),
        }
    }
}
pub fn encode_community_fact(fact: &CommunityFact) -> Result<Vec<u8>, CodecError> {
    let bytes = serde_json::to_vec(&Envelope {
        schema_version: VERSION,
        record_type: KIND.to_owned(),
        payload: StoredFact::from_domain(fact),
    })
    .map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    if bytes.len() > MAX_BYTES {
        return Err(CodecError::InvalidPayload(
            "community record exceeds 1MiB".to_owned(),
        ));
    }
    Ok(bytes)
}
pub fn decode_community_fact(bytes: &[u8]) -> Result<CommunityFact, CodecError> {
    if bytes.len() > MAX_BYTES {
        return Err(CodecError::InvalidPayload(
            "community record exceeds 1MiB".to_owned(),
        ));
    }
    let record: Envelope<StoredFact> =
        serde_json::from_slice(bytes).map_err(|e| CodecError::InvalidPayload(e.to_string()))?;
    if record.schema_version != VERSION {
        return Err(CodecError::UnsupportedVersion(record.schema_version));
    }
    if record.record_type != KIND {
        return Err(CodecError::UnsupportedRecordKind(record.record_type));
    }
    record.payload.checked_domain()
}
/// Replay ordered immutable registration facts with the original scoped Ledger.
/// No independent trusted stream ordering, completeness or durable transaction.
pub fn replay_community_facts<'a>(
    facts: impl IntoIterator<Item = &'a [u8]>,
    ledger: &Ledger,
) -> Result<CommunityRegistry, CodecError> {
    let mut registry = CommunityRegistry::new();
    let mut accepted: BTreeMap<(String, String), Vec<u8>> = BTreeMap::new();
    for bytes in facts {
        let fact = decode_community_fact(bytes)?;
        let (kind, id) = fact.identity();
        let key = (kind.to_owned(), id.to_owned());
        let canonical = encode_community_fact(&fact)?;
        if let Some(previous) = accepted.get(&key) {
            if previous != &canonical {
                return Err(CodecError::Replay(
                    "changed community fact reused identity".to_owned(),
                ));
            }
            continue;
        }
        let result = match fact {
            CommunityFact::Organization(v) => registry.register_organization(v),
            CommunityFact::Party(v) => registry.register_party(v),
            CommunityFact::Community(v) => registry.register_community(v),
            CommunityFact::Membership(v) => registry.register_membership(v),
            CommunityFact::Fund(v) => registry.register_fund(v, ledger),
        };
        result.map_err(|e| CodecError::Replay(e.to_string()))?;
        accepted.insert(key, canonical);
    }
    Ok(registry)
}
