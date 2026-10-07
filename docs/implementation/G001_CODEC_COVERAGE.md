# G001 codec coverage — first ledger slice

Status: **PARTIAL IMPLEMENTATION / NOT G001 QUALIFIED**.

This file records the real scope of the first code slice in issue #22. G001 requires
**all** economic and authority registries, not just the journal. No database,
API, provider execution, or live-money capability is introduced here.

## Implemented now

| Canonical fact | Origin | Codec | Rebuild mechanism | Proof |
| --- | --- | --- | --- | --- |
| Account registration | `cofi-ledger::Account` | `ledger.account`, v1 | `AccountId::new`, `LedgerScopeId::new`, `Currency::new`, `Account::new`, then `Ledger::register_account` | Checked roundtrip; account parity |
| Journal entry and ordered postings | `cofi-ledger::JournalEntry` | `ledger.entry`, v1 | `Posting::new`, `EntryMetadata::new`, `JournalEntry::new`, then `Ledger::commit` | i128 boundary; exact timestamps/metadata/order; replay, conflict, missing account |
| Replay/business-key indexes | `cofi-ledger::Ledger` | Derived, never serialized directly | Rebuilt by `Ledger::commit` | Identical replay and changed-key conflict |
| Debit/credit balance projections | `cofi-ledger::Ledger` | Derived, never serialized directly | Recomputed using domain postings and checked `u128` arithmetic | Reference balance comparison; decimal `u128::MAX` codec |

Record payloads use typed JSON with required version/type, unknown-field rejection,
canonical decimal **strings** for monetary amounts and timestamps, and a checked
constructor-only rehydration path. Values are not converted through floating point.

The current ledger fact stream requires account records before dependent entries.
An independently determined canonical ordering and explicit provenance/sequence
will be specified before G003. Record type/version rejection is fail-closed.

## Explicitly not yet implemented

| Current crate(s) | Missing G001 accepted-fact/replay coverage |
| --- | --- |
| `cofi-community` | organizations, memberships, shared funds, allocations, transfers and distributions |
| `cofi-governance`, `cofi-spending` | proposals, approvals, quorum, consumed authority and approved spending |
| `cofi-disbursements`, `cofi-provider-contract` | lifecycle, request, observation and evidence bindings |
| `cofi-reconciliation`, `cofi-audit` | cases/outcomes and authenticated audit streams |
| `cofi-metering`, `cofi-rating` | usage events/aggregation, pricing plans and rating results |
| `cofi-billing`, `cofi-invoicing`, `cofi-subscriptions` | customers, receivables, invoice lifecycle and subscription schedule/indexes |
| `cofi-payments` | capture and payout accounting source events |
| `cofi-rating-authorization`, `cofi-invoice-authorization`, `cofi-finalization-authorization` | lineage, immutable authorization results and consumed keys |
| `cofi-payment-authorization`, `cofi-payout-authorization` | capture/payout authorization and exact source-event binding |
| `cofi-fund-allocation-authorization`, `cofi-fund-transfer-authorization` | fund movement authorization/budget lineage |

This summary is a coverage *frontier*, not a proof of complete domain transitions.
For each outstanding registry, map: constructor and all accepted commands, immutable
facts, resulting indexes, exact identities, timestamps/sequence, dependencies,
snapshot references, conflicts and state-read APIs. Record any missing public
getter/constructor as an explicit blocker rather than deserializing private state.

## G001 closure gates

1. Enumerate every accepted current transition/result across all 21 original crates.
2. Add checked versioned codecs and replay adapters without new money algorithms.
3. Compare independently replayed complete state, results and rejection cases
   against in-memory reference fixtures at extreme numeric/temporal boundaries.
4. Prove changed duplicate IDs/business keys, invalid versions, missing ancestry
   and corrupted payloads fail closed across **all** registries.
5. Exact-head fmt, Clippy, workspace tests, MSRV, identity and actual Jev/OCR reviews.
6. Merge only after all scope requirements are satisfied or split and track remaining
   grain(s) without claiming the parent G001 is complete.

## Safety constraints

No new production acceptance boundary exists until G002–G008 and their gates pass.
No network effects may run during replay or retry. Account balances and authority
indexes must not be populated from unchecked persistence DTOs.
