# G001 codec coverage — ledger, metering and rating evidence slices

Status: **PARTIAL IMPLEMENTATION / NOT G001 QUALIFIED**.

This file records the actual delivered codec slices in issue #22. G001 requires
**all** economic and authority registries, not just ledger, metering and rating. No database,
API, provider execution, or live-money capability is introduced here.

## Implemented now

| Canonical fact | Origin | Codec | Rebuild mechanism | Proof |
| --- | --- | --- | --- | --- |
| Account registration | `cofi-ledger::Account` | `ledger.account`, v1 | `AccountId::new`, `LedgerScopeId::new`, `Currency::new`, `Account::new`, then `Ledger::register_account` | Checked roundtrip; account parity |
| Journal entry and ordered postings | `cofi-ledger::JournalEntry` | `ledger.entry`, v1 | `Posting::new`, `EntryMetadata::new`, `JournalEntry::new`, then `Ledger::commit` | i128 boundary; exact timestamps/metadata/order; replay, conflict, missing account |
| Replay/business-key indexes | `cofi-ledger::Ledger` | Derived, never serialized directly | Rebuilt by `Ledger::commit` | Identical replay and changed-key conflict |
| Debit/credit balance projections | `cofi-ledger::Ledger` | Derived, never serialized directly | Recomputed using domain postings and checked `u128` arithmetic | Reference balance comparison; decimal `u128::MAX` codec |
| Meter definition | `cofi-metering::MeterDefinition` | `meter.definition`, v1 | `MeterId::new`, `EventType::new`, `MeterDefinition::new`, `MeteringEngine::register_meter` | Full definition roundtrip, replay and changed-identity conflict |
| Usage event (Count/Sum) | `cofi-metering::UsageEvent` | `meter.event`, v1 | Checked IDs, times and decimal-string `i128` quantity, `UsageEvent::new`, `MeteringEngine::ingest` | Full event parity, source-event replay, type/shape/timestamp and missing-parent failure |
| Usage aggregate | `cofi-metering::UsageAggregate` | Derived, never serialized directly | Recomputed by `MeteringEngine::aggregate` | Sum `i128::MAX`, Count, exact reference parity |
| Rate plan snapshot | cofi-rating::RatePlan | rate.plan v1 | Checked RatePlan constructors, all exact integer fields | i128 bounds, malformed plan and price rejection |
| Historical accepted rating | cofi-rating::RatingRequest / RatedCharge | rating.acceptance v1 | Frozen original meter/usage/plan; canonical aggregate and rate; verify every charge field | Late-arrival nonretroactivity, altered source, mismatched receipt, identity/key conflict, overflow |

Record payloads use typed JSON with required version/type, unknown-field rejection,
canonical decimal **strings** for monetary amounts and timestamps, and a checked
constructor-only rehydration path. Values are not converted through floating point.

The ledger fact stream requires accounts before dependent entries. The separate
metering fact stream requires meter definitions before dependent usage events.
Both reject changed replay identities; neither currently supplies a durable
transactional sequence, digest, tenant boundary or production acceptance.
The metering domain does not encode organization scope on its facts; tenant
isolation must be structurally enforced at the eventual G002 storage boundary.

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
| cofi-rating production admission | Authenticated complete source-event cutoff, tenant scope and authorization lineage remain unproven |
| `cofi-billing`, `cofi-invoicing`, `cofi-subscriptions` | customers, receivables, invoice lifecycle and subscription schedule/indexes |
| `cofi-payments` | capture and payout accounting source events |
| `cofi-rating-authorization`, `cofi-invoice-authorization`, `cofi-finalization-authorization` | lineage, immutable authorization results and consumed keys |
| `cofi-payment-authorization`, `cofi-payout-authorization` | capture/payout authorization and exact source-event binding |
| `cofi-fund-allocation-authorization`, `cofi-fund-transfer-authorization` | fund movement authorization/budget lineage |

### Historical rating evidence limitation

The rating.acceptance codec checks a supplied **frozen source-event set**
against its original meter definition, price plan and original charge.
A later-arriving event cannot be silently used to change that receipt.
However, a codec alone **cannot prove completeness** of the supplied source
events at the historical acceptance boundary. Coherent tampering of both
event snapshots and charge is not cryptographically detectable here.
A durable canonical sequence/checkpoint, authenticated source provenance,
tenant/scope enforcement and atomic binding to accepted rating must be
implemented in G002-G004 before production replay or real billing.
The source domain currently carries no independent scope identity.

Pstack TDD evidence: the missing rating module/dependencies failed first.
Six targeted tests now verify late-arrival stability, exact price-plan
roundtrip, source/receipt corruption rejection, changed equal-value source,
duplicate rating key and multiplication overflow. Graft dependency analysis
must accompany the exact diff before it can be merged.

This summary is a coverage *frontier*, not a proof of complete domain transitions.
The metering slice uses Pstack's narrow TDD/review workflow: first the missing-module
test failed as expected, then a test caught a hidden Count-with-Sum-fields
deserialization acceptance, and the production codec was corrected so it rejects
the malformed variant. The `cofi-metering` source and downstream rating,
invoicing and authorization call sites were inspected using a Graft wiring graph,
not inferred from repository names. Full local workspace tests/fmt/Clippy passed,
but final-head GitHub CI and Jev/OCR still govern branch promotion.

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
