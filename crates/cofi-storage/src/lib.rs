//! Validated, versioned persistence records.
//!
//! This first G001 slice only admits ledger accounts and journal entries.
//! Other economic and authorization registries remain blocked until dedicated
//! codecs, complete inventories, and parity proofs are reviewed.

pub mod metering;

use std::error::Error;
use std::fmt::{Display, Formatter};

use cofi_ledger::{
    Account, AccountId, AccountKind, Currency, EntryMetadata, JournalEntry, JournalEntryId, Ledger,
    LedgerScopeId, Posting, Side,
};
use serde::{Deserialize, Serialize};

const CURRENT_VERSION: u64 = 1;
const ACCOUNT_KIND: &str = "ledger.account";
const ENTRY_KIND: &str = "ledger.entry";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodecError {
    InvalidPayload(String),
    UnsupportedVersion(u64),
    UnsupportedRecordKind(String),
    InvalidInteger(&'static str),
    InvalidDomain(String),
    Replay(String),
}

impl Display for CodecError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPayload(reason) => write!(f, "invalid persistence payload: {reason}"),
            Self::UnsupportedVersion(version) => {
                write!(f, "unsupported persistence record version: {version}")
            }
            Self::UnsupportedRecordKind(kind) => {
                write!(f, "unsupported persistence record kind: {kind}")
            }
            Self::InvalidInteger(field) => write!(f, "noncanonical integer in field {field}"),
            Self::InvalidDomain(reason) => write!(f, "domain rejected persistence fact: {reason}"),
            Self::Replay(reason) => write!(f, "ledger replay rejected persistence fact: {reason}"),
        }
    }
}

impl Error for CodecError {}

/// No signs, fractions, whitespace, exponent notation, or padded zeros.
/// PostgreSQL NUMERIC ingress must preserve this exact canonical representation.
fn is_canonical_integer(value: &str, signed: bool) -> bool {
    let digits = if signed && value.starts_with('-') {
        &value.as_bytes()[1..]
    } else {
        value.as_bytes()
    };
    !digits.is_empty()
        && digits.iter().all(u8::is_ascii_digit)
        && (digits.len() == 1 || digits[0] != b'0')
        && !(value.starts_with('-') && digits == b"0")
}

/// Exact u128 values (including balance totals) must not transit f64 or JSON numbers.
pub fn parse_u128_exact(value: &str) -> Result<u128, CodecError> {
    if !is_canonical_integer(value, false) {
        return Err(CodecError::InvalidInteger("u128"));
    }
    value
        .parse()
        .map_err(|_| CodecError::InvalidInteger("u128"))
}

pub fn parse_i128_exact(value: &str) -> Result<i128, CodecError> {
    if !is_canonical_integer(value, true) {
        return Err(CodecError::InvalidInteger("i128"));
    }
    value
        .parse()
        .map_err(|_| CodecError::InvalidInteger("i128"))
}

pub fn parse_i64_exact(value: &str) -> Result<i64, CodecError> {
    if !is_canonical_integer(value, true) {
        return Err(CodecError::InvalidInteger("i64"));
    }
    value.parse().map_err(|_| CodecError::InvalidInteger("i64"))
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope<T> {
    schema_version: u64,
    record_type: String,
    payload: T,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum StoredAccountKind {
    Asset,
    Liability,
    Equity,
    Revenue,
    Expense,
}

impl From<AccountKind> for StoredAccountKind {
    fn from(kind: AccountKind) -> Self {
        match kind {
            AccountKind::Asset => Self::Asset,
            AccountKind::Liability => Self::Liability,
            AccountKind::Equity => Self::Equity,
            AccountKind::Revenue => Self::Revenue,
            AccountKind::Expense => Self::Expense,
        }
    }
}

impl From<StoredAccountKind> for AccountKind {
    fn from(kind: StoredAccountKind) -> Self {
        match kind {
            StoredAccountKind::Asset => Self::Asset,
            StoredAccountKind::Liability => Self::Liability,
            StoredAccountKind::Equity => Self::Equity,
            StoredAccountKind::Revenue => Self::Revenue,
            StoredAccountKind::Expense => Self::Expense,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AccountRecord {
    id: String,
    scope_id: String,
    kind: StoredAccountKind,
    currency: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum StoredSide {
    Debit,
    Credit,
}

impl From<Side> for StoredSide {
    fn from(side: Side) -> Self {
        match side {
            Side::Debit => Self::Debit,
            Side::Credit => Self::Credit,
        }
    }
}

impl From<StoredSide> for Side {
    fn from(side: StoredSide) -> Self {
        match side {
            StoredSide::Debit => Self::Debit,
            StoredSide::Credit => Self::Credit,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PostingRecord {
    account_id: String,
    currency: String,
    side: StoredSide,
    amount_minor: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct JournalRecord {
    id: String,
    postings: Vec<PostingRecord>,
    effective_at_unix_ms: String,
    recorded_at_unix_ms: String,
    correlation_id: Option<String>,
    idempotency_key: Option<String>,
    business_key: Option<String>,
}

pub enum LedgerFact {
    Account(Account),
    Entry(JournalEntry),
}

fn encode<T: Serialize>(kind: &str, payload: T) -> Result<Vec<u8>, CodecError> {
    serde_json::to_vec(&Envelope {
        schema_version: CURRENT_VERSION,
        record_type: kind.to_owned(),
        payload,
    })
    .map_err(|err| CodecError::InvalidPayload(err.to_string()))
}

pub fn encode_account(account: &Account) -> Result<Vec<u8>, CodecError> {
    encode(
        ACCOUNT_KIND,
        AccountRecord {
            id: account.id().as_str().to_owned(),
            scope_id: account.scope_id().as_str().to_owned(),
            kind: account.kind().into(),
            currency: account.currency().code().to_owned(),
        },
    )
}

pub fn encode_entry(entry: &JournalEntry) -> Result<Vec<u8>, CodecError> {
    encode(
        ENTRY_KIND,
        JournalRecord {
            id: entry.id().as_str().to_owned(),
            postings: entry
                .postings()
                .iter()
                .map(|posting| PostingRecord {
                    account_id: posting.account_id().as_str().to_owned(),
                    currency: posting.currency().code().to_owned(),
                    side: posting.side().into(),
                    amount_minor: posting.amount().value().to_string(),
                })
                .collect(),
            effective_at_unix_ms: entry.effective_at_unix_ms().to_string(),
            recorded_at_unix_ms: entry.recorded_at_unix_ms().to_string(),
            correlation_id: entry.metadata().correlation_id().map(str::to_owned),
            idempotency_key: entry.metadata().idempotency_key().map(str::to_owned),
            business_key: entry.metadata().business_key().map(str::to_owned),
        },
    )
}

/// Reject unknown fact types and versions, then route all decoded values
/// through the domain's existing checked public constructors.
pub fn decode_fact(bytes: &[u8]) -> Result<LedgerFact, CodecError> {
    let header: Envelope<serde_json::Value> =
        serde_json::from_slice(bytes).map_err(|err| CodecError::InvalidPayload(err.to_string()))?;
    if header.schema_version != CURRENT_VERSION {
        return Err(CodecError::UnsupportedVersion(header.schema_version));
    }

    match header.record_type.as_str() {
        ACCOUNT_KIND => {
            let record: Envelope<AccountRecord> = serde_json::from_slice(bytes)
                .map_err(|err| CodecError::InvalidPayload(err.to_string()))?;
            let account = Account::new(
                AccountId::new(record.payload.id)
                    .map_err(|err| CodecError::InvalidDomain(err.to_string()))?,
                LedgerScopeId::new(record.payload.scope_id)
                    .map_err(|err| CodecError::InvalidDomain(err.to_string()))?,
                record.payload.kind.into(),
                Currency::new(&record.payload.currency)
                    .map_err(|err| CodecError::InvalidDomain(err.to_string()))?,
            );
            Ok(LedgerFact::Account(account))
        }
        ENTRY_KIND => {
            let record: Envelope<JournalRecord> = serde_json::from_slice(bytes)
                .map_err(|err| CodecError::InvalidPayload(err.to_string()))?;
            let payload = record.payload;
            let postings = payload
                .postings
                .into_iter()
                .map(|p| {
                    Posting::new(
                        AccountId::new(p.account_id)
                            .map_err(|err| CodecError::InvalidDomain(err.to_string()))?,
                        Currency::new(&p.currency)
                            .map_err(|err| CodecError::InvalidDomain(err.to_string()))?,
                        p.side.into(),
                        parse_i128_exact(&p.amount_minor)?,
                    )
                    .map_err(|err| CodecError::InvalidDomain(err.to_string()))
                })
                .collect::<Result<Vec<_>, CodecError>>()?;
            let metadata = EntryMetadata::new(payload.correlation_id, payload.idempotency_key)
                .and_then(|metadata| metadata.with_business_key(payload.business_key))
                .map_err(|err| CodecError::InvalidDomain(err.to_string()))?;
            let entry = JournalEntry::new(
                JournalEntryId::new(payload.id)
                    .map_err(|err| CodecError::InvalidDomain(err.to_string()))?,
                postings,
                parse_i64_exact(&payload.effective_at_unix_ms)?,
                parse_i64_exact(&payload.recorded_at_unix_ms)?,
                metadata,
            )
            .map_err(|err| CodecError::InvalidDomain(err.to_string()))?;
            Ok(LedgerFact::Entry(entry))
        }
        kind => Err(CodecError::UnsupportedRecordKind(kind.to_owned())),
    }
}

/// The input stream must be in canonical dependency order: account
/// registrations before entries. Indexes/balances are rebuilt by the domain.
pub fn replay_ledger<'a>(facts: impl IntoIterator<Item = &'a [u8]>) -> Result<Ledger, CodecError> {
    let mut ledger = Ledger::new();
    for fact in facts {
        match decode_fact(fact)? {
            LedgerFact::Account(account) => ledger
                .register_account(account)
                .map_err(|err| CodecError::Replay(err.to_string()))?,
            LedgerFact::Entry(entry) => {
                ledger
                    .commit(entry)
                    .map_err(|err| CodecError::Replay(err.to_string()))?;
            }
        }
    }
    Ok(ledger)
}
