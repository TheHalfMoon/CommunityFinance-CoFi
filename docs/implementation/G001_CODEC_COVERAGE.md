# G001 codec coverage — ledger, metering, rating, subscription, draft invoice and audit event slices

Status: **PARTIAL IMPLEMENTATION / NOT G001 QUALIFIED**.

This file records the actual delivered codec slices in issue #22. G001 requires
**all** economic and authority registries, not just ledger, metering, rating, subscriptions, draft invoices and audit events. No database,
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
| Base community registrations | `cofi-community::CommunityRegistry` | `community.fact` v1 | Rebuild organization, party, community, membership and fund via original checked register methods, requiring reconstructed `Ledger` for fund account/scope/currency/Asset validation | Six focused tests: roundtrip, reference indexes, exact retry, missing parents, conflicting pairs, ledger kind/scope, changed IDs, invalid types/version/fields and 1MiB bound |
| Accepted subscription request | cofi-subscriptions::SubscriptionRequest | subscription.create v1 | Checked IDs, organization scope, customer, subject, embedded immutable plan snapshot, original effective interval; SubscriptionRequest::new and SubscriptionRegistry::create | Exact request and schedule parity, adjacent valid periods, overlap rejection, source-event identity and temporal failures |
| Derived subscription indexes/status | cofi-subscriptions::SubscriptionRegistry | Derived, never persisted directly | Existing create, status_at and resolve_for_window | Reference and window boundary parity |
| Authorized rating from accepted subscription and frozen metering evidence | cofi-rating-authorization::AuthorizedRatingRegistry | authorized.rating v1 (cofi-storage) | Reconstruct original subscription.create and rating.acceptance via domain constructors and meters; reapply AuthorizedRatingRegistry::rate; compare source IDs, scoped plan and full original charge | Exact authorization replay, altered plan/scope/source membership/charge, conflicting same-ID subscription and missing history tests |
| Authorized invoice draft from accepted P21 rating evidence | cofi-invoice-authorization::AuthorizedDraftRegistry | authorized.draft v1 | Decode each original authorized.rating evidence through P21; reconstruct AuthorizedDraftRequest, use canonical AuthorizedDraftRegistry::assemble, derive draft and rated-charge binding indexes; verify invoice ID, scope/customer, total and each line charge ID | Exact authorization parity/idempotent replay, mutated P21 lineage, changed ID/scope/total, duplicate source use across invoices |
| Authorized invoice finalization from accepted P22 draft history | cofi-finalization-authorization::AuthorizedFinalizationRegistry | authorized.finalization v1 | Decode original accepted P22 authorized.draft, reconstruct AuthorizedFinalizationRequest, invoke original AuthorizedFinalizationRegistry::finalize, verify original invoice/scope/customer/total and finalization event/times | 3 tests: exact finalization/BillingEvent parity, tampered source/time/amount/ID and double finalization rejected |
| Accepted draft invoice with frozen rating-line receipts | cofi-invoicing::DraftInvoiceRequest | invoice.draft v1 | Reconstruct every RatedCharge through original rating.acceptance; exact original billing event, invoice, customer/scope and interval; checked DraftInvoiceRequest::new and DraftInvoiceRegistry::assemble | Two-line amount parity, duplicate/missing receipt, changed identity, scope, total and charge-bound conflict tests |
| Derived draft invoice indexes/charge bindings | cofi-invoicing::DraftInvoiceRegistry | Never persisted directly | Existing canonical assemble method, plus to_billing_event projection | Parity of invoices/events/charge binding count and replay semantics |
| Accepted reconciliation audit event | cofi-audit::AuditEvent | audit.reconciliation v1 (cofi-audit canonical codec; cofi-storage re-export) | Original audit position/attribution/times/action/resource/typed reconciliation projection; AuditEvent::new and recomputed canonical SHA-256 digest parity | 6 targeted tests: two interleaved streams, original event/digest parity, bad digest/IDs/time/version/type/payload, changed duplicate, missing ancestor, oversized and duplicate JSON keys |
| Audit stream/index/tail projections | cofi-audit::AuditLog | Derived only | Existing AuditLog::append, verify_stream, checked previous digest and sequence; no second hash algorithm | Original stream/event counts, tail and event parity |

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
| `cofi-community` remaining modules | Allocation, transfer and distribution acceptance, ledger effects and consumed authority still unqualified; base community registration has a bounded checked codec. |
| `cofi-governance`, `cofi-spending` | proposals, approvals, quorum, consumed authority and approved spending |
| `cofi-disbursements`, `cofi-provider-contract` | lifecycle, request, observation and evidence bindings |
| cofi-reconciliation and audit upstream provenance | Full original ReconciliationCase/Outcome/provider-evidence reconstruction, stream completeness/external root authentication, tenant-scoped immutable append ledger |
| cofi-rating production admission | Authenticated complete source-event cutoff, tenant scope and authorization lineage remain unproven |
| cofi-billing, finalization and remaining invoicing | Ledger posting/receivables, authorized invoice finalization and lifecycle, authenticated rating checkpoint, complete accepted commercial history and payment integration remain unqualified |
| `cofi-payments` | capture and payout accounting source events |
| `cofi-rating-authorization`, `cofi-invoice-authorization`, `cofi-finalization-authorization` remaining production admission | P21/P22/P23 replay source evidence exists; complete independently authenticated scope/consumed authority and durable result lineage remain unqualified. |
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

### Subscription replay limitations

The subscription.create record rebuilds the original accepted immutable request
through checked public constructors. Its registry derives schedules, duplicate
guards, status and window resolution; the embedded RatePlan is immutable.
A codec alone does not authenticate the full source-event history, trusted order,
tenant binding, or atomic persistence at acceptance. Those remain G002-G004
gates and prohibit production rating/billing activation.

Pstack RED-before-GREEN: unresolved codec/dependency compiler errors preceded
the implementation. Four focused tests now pass, covering request and schedule
parity, original plan, adjacent intervals, overlapping intervals and conflicts.
Graft blast-radius and exact-head reviews are required for merger.

### Draft-invoice evidence limits

The invoice.draft codec carries the exact original draft request data and
one immutable rating.acceptance evidence bundle per rated charge, then
recomputes every charge through the original RatingRegistry. It reconstructs
the draft through DraftInvoiceRequest and DraftInvoiceRegistry, preserving
original source-event, invoice, rated-charge binding and idempotent replay
semantics. Missing/extra evidence and mismatched totals reject.

This is **not** an authenticated complete billing ledger. The embedded rating
evidence cannot establish the true cutoff/completeness of the external usage
stream; a coherent forgery rewriting both upstream receipts and source
information still requires durable, signed/verified event provenance.
Invoice finalization and ledger posting are not part of this draft-only
codec. Tenant-scoped atomic stream acceptance is blocked until G002-G004.
Pstack RED-before-GREEN produced missing module/dependency errors, and the
focused draft roundtrip/replay/failure tests now pass; Graft and exact-head
review must qualify the slice before merge.

### Audit event reconstruction limits

The audit.reconciliation record is implemented **in cofi-audit**, where
canonical SHA-256 digest construction, existing private reconciliation payload
projection and stream checks already live. cofi-storage only re-exports the
checked codec. It reconstitutes an original event with AuditEvent::new,
compares every recorded digest against the canonical calculation, and replays
the provided source facts with AuditLog::append/verify_stream. Records are
strictly typed and 1 MiB limited individually; unsupported versions, invalid
hex, duplicate JSON keys, invalid payload-outcome shapes, missing ancestry,
sequence gaps and changed duplicate identities fail closed.

A second G001 slice adds typed caller-provided AuditStreamAnchor and
replay_audit_events_anchored, comparing **all and only** expected streams to
both their known terminal sequence and digest. This detects a truncated or
coherently rewritten chain when the original trusted anchor is retained
independently. The API **does not acquire, authenticate, sign, persist, or
assign tenant scope to the anchor**. A malicious caller who supplies a forged
matching digest defeats it. This remains a local verification primitive,
not a production source-of-truth or a completed G001 provenance chain.

This verifies consistency, not authenticity. A forged coherent chain
can compute entirely valid SHA-256 hashes. This first audit slice does not
persist a trusted digest anchor, external signatures, a verified tenant-bound
canonical provider/reconciliation event history, or proof that the source
supplied *all* events. The compact reconciliation audit projection does not
contain the full ProviderAhead terminal event, so it cannot reconstruct
a canonical source ReconciliationOutcome and provider input independently.
Full upstream lineage and verified source completeness are still unresolved
under issue #31 and G002-G008. No production audit claim is authorized.

Pstack tests were written before implementation and failed on the missing
codec/serde; after implementation the six focused tests and workspace lint
checks passed locally. Graft source graph/blast plus exact-head CI, Jev and
OCR accounting are required before normal merge.

### Authorized rating G001 slice boundary

The authorized.rating acceptance envelope binds the original immutable
subscription.create and frozen rating.acceptance source evidence and its
usage-event identifier list. Replay rebuilds the actual UsageAggregate from
original checked meter events and invokes the **original**
AuthorizedRatingRegistry::rate, which resolves the canonical subscription and
produces a scoped authorized charge. Source identities, plan/price, scope,
event ID, amount and accepted charge must match the recorded snapshot.
Duplicate identical replay is a no-op; a changed subscription or rating
source under the same rating-event ID fails closed.

**Trust limit:** Both embedded sources and the expected event-ID list live
inside the *same* untrusted record. An attacker who rewrites all of them
coherently may still pass internal checks. The accepted-history record itself
does not authenticate the authoritative organization scope, exact eligible
usage-event completeness, tamper-resistant external source roots, or atomic
tenant-specific acceptance. G002-G004 must bind these independently to
trusted storage and the canonical source cutoff. P22 authorized drafts and
P23 authorized finalization remain separate *unimplemented* source-registry
codecs. This slice is not production-eligible billing.

Pstack TDD found an equal-amount changed-source-ID acceptance and the
auth record now binds the exact original event-ID list. Four tests cover
source ancestry/replay and negative corruption. Graft diff analysis and
exact-head CI/Jev/OCR are required before merging this slice.

### Authorized P22 draft source ancestry

The `authorized.draft` v1 record contains the original accepted P21
authorized.rating source receipt per rated line. Rehydration recomputes
each AuthorizedRating through the P21 domain registry, reconstructs the
original AuthorizedDraftRequest, and invokes only the canonical
AuthorizedDraftRegistry::assemble to recover accepted draft, event and
rated-charge binding indexes. Original billed invoice ID, scope/customer,
total and line IDs are compared to recorded acceptance; exact replay is
idempotent while a changed P21 evidence or re-used charge in another
invoice fails closed.

**Authenticity remains an external obligation:** the embedded expected
values and all P21 source evidence are in the same untrusted v1 envelope,
not independently authenticated. A coherent forged record can pass
internal parity. Source-event completeness, tenant-scoped trusted
sequence/transaction, and independently trusted P23 source provenance remain OPEN.
This is not a released or production-authorized billing workflow.

The Pstack-style RED-before-GREEN test first reported missing P22 codec,
then exposed a mutable invoice ID not independently checked even within
the same envelope; the bounded partial-tamper guard was added. Three
focused tests exercise accepted result parity and negative mutation
and charge reuse; exact-head CI/Jev/OCR and Graft review are mandatory.

### P23 authorized invoice finalization

The authorized.finalization v1 acceptance record carries original P22
authorized.draft evidence, original source finalization event, times and
expected original invoice/scope/customer/total. Decoding P23 first
reconstructs the actual original AuthorizedDraft via P21 and P22 domain
registries, then uses **only** AuthorizedFinalizationRegistry::finalize
to rederive the original FinalizedInvoice and its projected InvoiceEvent.
No direct FinalizedInvoice construction, unverified authorization DTO,
new revenue posting algorithm, database mutation or network call occurs.

Same invoice cannot be finalized twice, changed same finalization source
event cannot silently replay, and changed source evidence/time/total
fails closed. Three focused P23 tests check exact ancestry and replay,
negative corruption and duplicate invoice/finalization rejection.

**The trusted source boundary remains open.** Internally consistent
source evidence still cannot prove authenticity or completeness, and the
P23 projection is not an external authorization certificate. There is
no tenant-scoped atomic transaction, durable ledger posting or money
movement. BillingLedgerBridge remains separate and MUST NOT execute
during hydration. G002-G008 remain hard gates.

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

### Community base registration replay boundary

The `community.fact` v1 codec preserves the original accepted
organization, party, community, membership and fund registration facts
in supplied order. Only original domain constructors and CommunityRegistry
registration methods rebuild lookup indexes and parent associations.
Fund reconstruction requires the separately rehydrated Ledger and validates
Asset account kind, organization scope, currency and unique account binding.
Identical repeated records are idempotent; changed identity is rejected.

This slice does **not** cover community allocations, transfers or
distributions; it does not authenticate the supplied event stream, prove
that it includes every accepted event, or atomically durably persist facts
under a trusted tenant identity. G001 remains PARTIAL and G002-G008
production-eligibility gates remain blocked.

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
