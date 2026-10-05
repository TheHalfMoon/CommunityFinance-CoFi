use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use cofi_ledger::{AccountId, AccountKind, Currency, Ledger, LedgerScopeId};

mod allocation;
pub use allocation::*;
mod transfer;
pub use transfer::*;
mod distribution;
pub use distribution::*;

macro_rules! domain_id {
    ($name:ident, $label:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, CommunityError> {
                let value = value.into();
                if value.trim().is_empty() {
                    return Err(CommunityError::EmptyIdentifier($label));
                }
                Ok(Self(value))
            }

            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}
domain_id!(OrganizationId, "organization_id");
domain_id!(PartyId, "party_id");
domain_id!(CommunityId, "community_id");
domain_id!(MembershipId, "membership_id");
domain_id!(FundId, "fund_id");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Organization {
    id: OrganizationId,
}

impl Organization {
    #[must_use]
    pub const fn new(id: OrganizationId) -> Self {
        Self { id }
    }

    #[must_use]
    pub const fn id(&self) -> &OrganizationId {
        &self.id
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartyKind {
    Person,
    Organization,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Party {
    id: PartyId,
    organization_id: OrganizationId,
    kind: PartyKind,
}

impl Party {
    #[must_use]
    pub const fn new(id: PartyId, organization_id: OrganizationId, kind: PartyKind) -> Self {
        Self {
            id,
            organization_id,
            kind,
        }
    }

    #[must_use]
    pub const fn id(&self) -> &PartyId {
        &self.id
    }

    #[must_use]
    pub const fn organization_id(&self) -> &OrganizationId {
        &self.organization_id
    }

    #[must_use]
    pub const fn kind(&self) -> PartyKind {
        self.kind
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Community {
    id: CommunityId,
    organization_id: OrganizationId,
}

impl Community {
    #[must_use]
    pub const fn new(id: CommunityId, organization_id: OrganizationId) -> Self {
        Self {
            id,
            organization_id,
        }
    }

    #[must_use]
    pub const fn id(&self) -> &CommunityId {
        &self.id
    }

    #[must_use]
    pub const fn organization_id(&self) -> &OrganizationId {
        &self.organization_id
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MembershipRole {
    Owner,
    Admin,
    Treasurer,
    Member,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MembershipStatus {
    Active,
    Suspended,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Membership {
    id: MembershipId,
    party_id: PartyId,
    community_id: CommunityId,
    role: MembershipRole,
    status: MembershipStatus,
}

impl Membership {
    #[must_use]
    pub const fn new(
        id: MembershipId,
        party_id: PartyId,
        community_id: CommunityId,
        role: MembershipRole,
        status: MembershipStatus,
    ) -> Self {
        Self {
            id,
            party_id,
            community_id,
            role,
            status,
        }
    }
    #[must_use]
    pub const fn id(&self) -> &MembershipId {
        &self.id
    }

    #[must_use]
    pub const fn party_id(&self) -> &PartyId {
        &self.party_id
    }

    #[must_use]
    pub const fn community_id(&self) -> &CommunityId {
        &self.community_id
    }

    #[must_use]
    pub const fn role(&self) -> MembershipRole {
        self.role
    }

    #[must_use]
    pub const fn status(&self) -> MembershipStatus {
        self.status
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fund {
    id: FundId,
    community_id: CommunityId,
    ledger_account_id: AccountId,
    currency: Currency,
}
impl Fund {
    #[must_use]
    pub const fn new(
        id: FundId,
        community_id: CommunityId,
        ledger_account_id: AccountId,
        currency: Currency,
    ) -> Self {
        Self {
            id,
            community_id,
            ledger_account_id,
            currency,
        }
    }

    #[must_use]
    pub const fn id(&self) -> &FundId {
        &self.id
    }

    #[must_use]
    pub const fn community_id(&self) -> &CommunityId {
        &self.community_id
    }

    #[must_use]
    pub const fn ledger_account_id(&self) -> &AccountId {
        &self.ledger_account_id
    }

    #[must_use]
    pub const fn currency(&self) -> Currency {
        self.currency
    }
}
#[derive(Debug, Clone, Default)]
pub struct CommunityRegistry {
    organizations: BTreeMap<OrganizationId, Organization>,
    parties: BTreeMap<PartyId, Party>,
    communities: BTreeMap<CommunityId, Community>,
    memberships: BTreeMap<MembershipId, Membership>,
    membership_pairs: BTreeMap<(CommunityId, PartyId), MembershipId>,
    funds: BTreeMap<FundId, Fund>,
    fund_accounts: BTreeMap<AccountId, FundId>,
}

impl CommunityRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_organization(
        &mut self,
        organization: Organization,
    ) -> Result<(), CommunityError> {
        if self.organizations.contains_key(organization.id()) {
            return Err(CommunityError::DuplicateOrganization(
                organization.id().clone(),
            ));
        }
        self.organizations
            .insert(organization.id().clone(), organization);
        Ok(())
    }

    pub fn register_party(&mut self, party: Party) -> Result<(), CommunityError> {
        if self.parties.contains_key(party.id()) {
            return Err(CommunityError::DuplicateParty(party.id().clone()));
        }
        if !self.organizations.contains_key(party.organization_id()) {
            return Err(CommunityError::UnknownOrganization(
                party.organization_id().clone(),
            ));
        }
        self.parties.insert(party.id().clone(), party);
        Ok(())
    }

    pub fn register_community(&mut self, community: Community) -> Result<(), CommunityError> {
        if self.communities.contains_key(community.id()) {
            return Err(CommunityError::DuplicateCommunity(community.id().clone()));
        }
        if !self.organizations.contains_key(community.organization_id()) {
            return Err(CommunityError::UnknownOrganization(
                community.organization_id().clone(),
            ));
        }
        self.communities.insert(community.id().clone(), community);
        Ok(())
    }

    pub fn register_membership(&mut self, membership: Membership) -> Result<(), CommunityError> {
        if self.memberships.contains_key(membership.id()) {
            return Err(CommunityError::DuplicateMembershipId(
                membership.id().clone(),
            ));
        }
        let party = self
            .parties
            .get(membership.party_id())
            .ok_or_else(|| CommunityError::UnknownParty(membership.party_id().clone()))?;
        let community = self
            .communities
            .get(membership.community_id())
            .ok_or_else(|| CommunityError::UnknownCommunity(membership.community_id().clone()))?;
        if party.organization_id() != community.organization_id() {
            return Err(CommunityError::CrossOrganizationMembership {
                party_organization_id: party.organization_id().clone(),
                community_organization_id: community.organization_id().clone(),
            });
        }
        let pair = (
            membership.community_id().clone(),
            membership.party_id().clone(),
        );
        if self.membership_pairs.contains_key(&pair) {
            return Err(CommunityError::DuplicateMembership {
                community_id: membership.community_id().clone(),
                party_id: membership.party_id().clone(),
            });
        }

        let membership_id = membership.id().clone();
        self.membership_pairs.insert(pair, membership_id.clone());
        self.memberships.insert(membership_id, membership);
        Ok(())
    }

    pub fn register_fund(&mut self, fund: Fund, ledger: &Ledger) -> Result<(), CommunityError> {
        if self.funds.contains_key(fund.id()) {
            return Err(CommunityError::DuplicateFund(fund.id().clone()));
        }
        let community = self
            .communities
            .get(fund.community_id())
            .ok_or_else(|| CommunityError::UnknownCommunity(fund.community_id().clone()))?;
        let account = ledger.account(fund.ledger_account_id()).ok_or_else(|| {
            CommunityError::UnknownLedgerAccount(fund.ledger_account_id().clone())
        })?;
        if account.kind() != AccountKind::Asset {
            return Err(CommunityError::FundLedgerAccountKindMismatch {
                fund_id: fund.id().clone(),
                actual: account.kind(),
            });
        }
        if account.scope_id().as_str() != community.organization_id().as_str() {
            return Err(CommunityError::FundLedgerScopeMismatch {
                organization_id: community.organization_id().clone(),
                ledger_scope_id: account.scope_id().clone(),
            });
        }
        if account.currency() != fund.currency() {
            return Err(CommunityError::FundCurrencyMismatch {
                fund_id: fund.id().clone(),
                expected: account.currency(),
                actual: fund.currency(),
            });
        }
        if let Some(existing_fund_id) = self.fund_accounts.get(fund.ledger_account_id()) {
            return Err(CommunityError::LedgerAccountAlreadyBound {
                account_id: fund.ledger_account_id().clone(),
                fund_id: existing_fund_id.clone(),
            });
        }

        let fund_id = fund.id().clone();
        self.fund_accounts
            .insert(fund.ledger_account_id().clone(), fund_id.clone());
        self.funds.insert(fund_id, fund);
        Ok(())
    }

    #[must_use]
    pub fn organization(&self, id: &OrganizationId) -> Option<&Organization> {
        self.organizations.get(id)
    }

    #[must_use]
    pub fn party(&self, id: &PartyId) -> Option<&Party> {
        self.parties.get(id)
    }

    #[must_use]
    pub fn community(&self, id: &CommunityId) -> Option<&Community> {
        self.communities.get(id)
    }

    #[must_use]
    pub fn membership(&self, id: &MembershipId) -> Option<&Membership> {
        self.memberships.get(id)
    }

    #[must_use]
    pub fn fund(&self, id: &FundId) -> Option<&Fund> {
        self.funds.get(id)
    }

    #[must_use]
    pub fn membership_for(
        &self,
        community_id: &CommunityId,
        party_id: &PartyId,
    ) -> Option<&Membership> {
        let id = self
            .membership_pairs
            .get(&(community_id.clone(), party_id.clone()))?;
        self.memberships.get(id)
    }

    #[must_use]
    pub fn fund_for_ledger_account(&self, account_id: &AccountId) -> Option<&Fund> {
        let id = self.fund_accounts.get(account_id)?;
        self.funds.get(id)
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommunityError {
    EmptyIdentifier(&'static str),
    DuplicateOrganization(OrganizationId),
    DuplicateParty(PartyId),
    DuplicateCommunity(CommunityId),
    DuplicateMembershipId(MembershipId),
    DuplicateMembership {
        community_id: CommunityId,
        party_id: PartyId,
    },
    DuplicateFund(FundId),
    UnknownOrganization(OrganizationId),
    UnknownParty(PartyId),
    UnknownCommunity(CommunityId),
    UnknownLedgerAccount(AccountId),
    FundLedgerAccountKindMismatch {
        fund_id: FundId,
        actual: AccountKind,
    },
    CrossOrganizationMembership {
        party_organization_id: OrganizationId,
        community_organization_id: OrganizationId,
    },
    FundLedgerScopeMismatch {
        organization_id: OrganizationId,
        ledger_scope_id: LedgerScopeId,
    },
    FundCurrencyMismatch {
        fund_id: FundId,
        expected: Currency,
        actual: Currency,
    },
    LedgerAccountAlreadyBound {
        account_id: AccountId,
        fund_id: FundId,
    },
}

impl Display for CommunityError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyIdentifier(name) => write!(f, "{name} must not be empty"),
            Self::DuplicateOrganization(id) => {
                write!(f, "organization already registered: {}", id.as_str())
            }
            Self::DuplicateParty(id) => write!(f, "party already registered: {}", id.as_str()),
            Self::DuplicateCommunity(id) => {
                write!(f, "community already registered: {}", id.as_str())
            }
            Self::DuplicateMembershipId(id) => {
                write!(f, "membership ID already registered: {}", id.as_str())
            }
            Self::DuplicateMembership {
                community_id,
                party_id,
            } => write!(
                f,
                "party {} is already a member of community {}",
                party_id.as_str(),
                community_id.as_str()
            ),
            Self::DuplicateFund(id) => write!(f, "fund already registered: {}", id.as_str()),
            Self::UnknownOrganization(id) => {
                write!(f, "unknown organization: {}", id.as_str())
            }
            Self::UnknownParty(id) => write!(f, "unknown party: {}", id.as_str()),
            Self::UnknownCommunity(id) => write!(f, "unknown community: {}", id.as_str()),
            Self::UnknownLedgerAccount(id) => {
                write!(f, "unknown ledger account: {}", id.as_str())
            }
            Self::FundLedgerAccountKindMismatch { fund_id, actual } => write!(
                f,
                "fund {} requires an Asset ledger account; got {actual:?}",
                fund_id.as_str()
            ),
            Self::CrossOrganizationMembership {
                party_organization_id,
                community_organization_id,
            } => write!(
                f,
                "cross-organization membership is forbidden: party={}, community={}",
                party_organization_id.as_str(),
                community_organization_id.as_str()
            ),
            Self::FundLedgerScopeMismatch {
                organization_id,
                ledger_scope_id,
            } => write!(
                f,
                "fund organization {} does not match ledger scope {}",
                organization_id.as_str(),
                ledger_scope_id.as_str()
            ),
            Self::FundCurrencyMismatch {
                fund_id,
                expected,
                actual,
            } => write!(
                f,
                "fund {} requires currency {expected}, got {actual}",
                fund_id.as_str()
            ),
            Self::LedgerAccountAlreadyBound {
                account_id,
                fund_id,
            } => write!(
                f,
                "ledger account {} is already bound to fund {}",
                account_id.as_str(),
                fund_id.as_str()
            ),
        }
    }
}

impl Error for CommunityError {}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use cofi_ledger::{Account, AccountKind, LedgerScopeId};

    fn org_id(value: &str) -> OrganizationId {
        OrganizationId::new(value).unwrap()
    }

    fn party_id(value: &str) -> PartyId {
        PartyId::new(value).unwrap()
    }

    fn community_id(value: &str) -> CommunityId {
        CommunityId::new(value).unwrap()
    }

    fn membership_id(value: &str) -> MembershipId {
        MembershipId::new(value).unwrap()
    }

    fn fund_id(value: &str) -> FundId {
        FundId::new(value).unwrap()
    }

    fn usd() -> Currency {
        Currency::new("USD").unwrap()
    }

    fn eur() -> Currency {
        Currency::new("EUR").unwrap()
    }

    fn ledger_with_account(scope: &str, account: &str, currency: Currency) -> Ledger {
        let mut ledger = Ledger::new();
        ledger
            .register_account(Account::new(
                AccountId::new(account).unwrap(),
                LedgerScopeId::new(scope).unwrap(),
                AccountKind::Asset,
                currency,
            ))
            .unwrap();
        ledger
    }

    fn base_registry() -> CommunityRegistry {
        let mut registry = CommunityRegistry::new();
        registry
            .register_organization(Organization::new(org_id("org-1")))
            .unwrap();
        registry
            .register_party(Party::new(
                party_id("party-1"),
                org_id("org-1"),
                PartyKind::Person,
            ))
            .unwrap();
        registry
            .register_community(Community::new(community_id("community-1"), org_id("org-1")))
            .unwrap();
        registry
    }

    #[test]
    fn registers_membership_and_fund_with_same_organization_scope() {
        let mut registry = base_registry();
        registry
            .register_membership(Membership::new(
                membership_id("membership-1"),
                party_id("party-1"),
                community_id("community-1"),
                MembershipRole::Owner,
                MembershipStatus::Active,
            ))
            .unwrap();
        let ledger = ledger_with_account("org-1", "community-fund", usd());
        let fund = Fund::new(
            fund_id("fund-1"),
            community_id("community-1"),
            AccountId::new("community-fund").unwrap(),
            usd(),
        );
        registry.register_fund(fund.clone(), &ledger).unwrap();
        assert_eq!(registry.fund(fund.id()), Some(&fund));
        assert!(
            registry
                .membership(&membership_id("membership-1"))
                .is_some()
        );
    }

    #[test]
    fn unknown_parent_registration_is_rejected() {
        let mut registry = CommunityRegistry::new();
        let party = Party::new(
            party_id("party-1"),
            org_id("missing-org"),
            PartyKind::Person,
        );
        assert_eq!(
            registry.register_party(party),
            Err(CommunityError::UnknownOrganization(org_id("missing-org")))
        );
        assert!(registry.party(&party_id("party-1")).is_none());

        let community = Community::new(community_id("community-1"), org_id("missing-org"));
        assert_eq!(
            registry.register_community(community),
            Err(CommunityError::UnknownOrganization(org_id("missing-org")))
        );
        assert!(registry.community(&community_id("community-1")).is_none());
    }

    #[test]
    fn cross_organization_membership_is_rejected_without_mutation() {
        let mut registry = base_registry();
        registry
            .register_organization(Organization::new(org_id("org-2")))
            .unwrap();
        registry
            .register_community(Community::new(community_id("community-2"), org_id("org-2")))
            .unwrap();
        let membership = Membership::new(
            membership_id("membership-x"),
            party_id("party-1"),
            community_id("community-2"),
            MembershipRole::Member,
            MembershipStatus::Active,
        );
        assert_eq!(
            registry.register_membership(membership),
            Err(CommunityError::CrossOrganizationMembership {
                party_organization_id: org_id("org-1"),
                community_organization_id: org_id("org-2"),
            })
        );
        assert!(
            registry
                .membership(&membership_id("membership-x"))
                .is_none()
        );
    }

    #[test]
    fn duplicate_membership_pair_is_rejected() {
        let mut registry = base_registry();
        let first = Membership::new(
            membership_id("membership-1"),
            party_id("party-1"),
            community_id("community-1"),
            MembershipRole::Member,
            MembershipStatus::Active,
        );
        registry.register_membership(first).unwrap();
        let duplicate = Membership::new(
            membership_id("membership-2"),
            party_id("party-1"),
            community_id("community-1"),
            MembershipRole::Admin,
            MembershipStatus::Active,
        );
        assert_eq!(
            registry.register_membership(duplicate),
            Err(CommunityError::DuplicateMembership {
                community_id: community_id("community-1"),
                party_id: party_id("party-1"),
            })
        );
        assert!(
            registry
                .membership(&membership_id("membership-2"))
                .is_none()
        );
    }

    #[test]
    fn fund_requires_existing_ledger_account() {
        let mut registry = base_registry();
        let ledger = Ledger::new();
        let fund = Fund::new(
            fund_id("fund-1"),
            community_id("community-1"),
            AccountId::new("missing").unwrap(),
            usd(),
        );
        assert_eq!(
            registry.register_fund(fund, &ledger),
            Err(CommunityError::UnknownLedgerAccount(
                AccountId::new("missing").unwrap()
            ))
        );
        assert!(registry.fund(&fund_id("fund-1")).is_none());
    }

    #[test]
    fn fund_cannot_cross_organization_ledger_scope() {
        let mut registry = base_registry();
        let ledger = ledger_with_account("org-2", "foreign-fund", usd());
        let fund = Fund::new(
            fund_id("fund-1"),
            community_id("community-1"),
            AccountId::new("foreign-fund").unwrap(),
            usd(),
        );
        assert_eq!(
            registry.register_fund(fund, &ledger),
            Err(CommunityError::FundLedgerScopeMismatch {
                organization_id: org_id("org-1"),
                ledger_scope_id: LedgerScopeId::new("org-2").unwrap(),
            })
        );
        assert!(registry.fund(&fund_id("fund-1")).is_none());
    }

    #[test]
    fn fund_currency_must_match_ledger_account() {
        let mut registry = base_registry();
        let ledger = ledger_with_account("org-1", "fund-account", usd());
        let fund = Fund::new(
            fund_id("fund-1"),
            community_id("community-1"),
            AccountId::new("fund-account").unwrap(),
            eur(),
        );
        assert_eq!(
            registry.register_fund(fund, &ledger),
            Err(CommunityError::FundCurrencyMismatch {
                fund_id: fund_id("fund-1"),
                expected: usd(),
                actual: eur(),
            })
        );
        assert!(registry.fund(&fund_id("fund-1")).is_none());
    }

    #[test]
    fn one_ledger_account_cannot_back_two_funds() {
        let mut registry = base_registry();
        let ledger = ledger_with_account("org-1", "fund-account", usd());
        let first = Fund::new(
            fund_id("fund-1"),
            community_id("community-1"),
            AccountId::new("fund-account").unwrap(),
            usd(),
        );
        registry.register_fund(first, &ledger).unwrap();
        let second = Fund::new(
            fund_id("fund-2"),
            community_id("community-1"),
            AccountId::new("fund-account").unwrap(),
            usd(),
        );
        assert_eq!(
            registry.register_fund(second, &ledger),
            Err(CommunityError::LedgerAccountAlreadyBound {
                account_id: AccountId::new("fund-account").unwrap(),
                fund_id: fund_id("fund-1"),
            })
        );
        assert!(registry.fund(&fund_id("fund-2")).is_none());
    }

    #[test]
    fn duplicate_domain_ids_are_rejected() {
        let mut registry = base_registry();
        assert_eq!(
            registry.register_organization(Organization::new(org_id("org-1"))),
            Err(CommunityError::DuplicateOrganization(org_id("org-1")))
        );
        assert_eq!(
            registry.register_party(Party::new(
                party_id("party-1"),
                org_id("org-1"),
                PartyKind::Person,
            )),
            Err(CommunityError::DuplicateParty(party_id("party-1")))
        );
        assert_eq!(
            registry
                .register_community(Community::new(community_id("community-1"), org_id("org-1"),)),
            Err(CommunityError::DuplicateCommunity(community_id(
                "community-1"
            )))
        );
    }

    #[test]
    fn empty_domain_identifiers_are_rejected() {
        assert_eq!(
            OrganizationId::new("   "),
            Err(CommunityError::EmptyIdentifier("organization_id"))
        );
        assert_eq!(
            FundId::new(""),
            Err(CommunityError::EmptyIdentifier("fund_id"))
        );
    }
}
