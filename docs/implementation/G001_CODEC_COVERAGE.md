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
| Immutable revenue split rule | `cofi-community::RevenueSplitRule` | `community.revenue_split_rule` v1 | Rebuild checked sorted destination legs via `BasisPoints::new`, `RevenueSplitLeg::new` and original `RevenueSplitRule::new` | Three tests cover exact version/leg parity, 10,000 basis-point sum, duplicate destinations, invalid numeric/version, unknown/extra JSON fields and size limit |
| Staged multi-flow two-tenant complete P23–P27 shared payout/transfer authority | Original shared AuthorizedCaptureRegistry, AuthorizedPayoutRegistry, AuthorizedFundAllocationRegistry, AuthorizedFundTransferRegistry | Shared staged causal registry reconstruction (not a new record kind) | Verify multi-flow original P23–P25 on zero-journal genesis, then all P26 allocations and all P27 transfers in explicitly staged historical order on private Ledger; require actual first-time Committed/Created, match 3×N, 4×N, 5×N journal snapshots and involved original accounts/balances | 4 focused tests: two independently scoped complete five-journal flows, real shared consumed payout/allocation indexes, conflicting reconsumption, wrong tenant/fund/cash/amount/time and missing after-stage reference |
| Ordered multi-flow two-tenant P23→P24→P25 causal authorization (bounded complete-flow stream) | Original BillingLedgerBridge, AuthorizedCaptureRegistry and AuthorizedPayoutRegistry | Shared causal registry reconstruction, not new fact kind | Require contiguous ordered complete P25 receipts, original P23/P24 checked facts, zero-journal genesis; rebuild both ORIGINAL shared authority registries via Created and compare exact 3×N journals and touched accounts/balances with final reference | 3 focused tests: two tenants and six journal parity, duplicate/gapped/order/ref failures, source tampering and cross-tenant account mix |
| Causal P23→P27 original complete payout-to-fund-transfer authorization (one flow) | Original AuthorizedFundTransferRegistry and FundTransferBridge on P23–P26 causal predecessor | checked causal reconstruction (no new record kind) | Supply P25, P26 and typed P27 sources, trusted zero-journal genesis, original community, and exact accepted after-P25/P26/P27 snapshots; first-time Committed creates actual P27 allocation consumption registry; require five original journal entries and matching account balances | Three tests: real P27 accepted authority and indexes, second transfer of same allocation rejected, wrong source/destination/funds/scope/amount/timing and missing intermediate/last journal |
| Causal P23/P24/P25 payout → P26 authorized full fund allocation (one flow) | Original AuthorizedFundAllocationRegistry / FundAllocationBridge plus causal P23–P25 | checked causal reconstruction (no new record kind) | Accept typed P26 source, exact separately supplied after-P25 and after-P26 reference Ledgers, original CommunityRegistry; original first-time Committed allocation consumes reconstructed payout authority, then match four original journals, original accounts and balances | 4 tests: original consumed-payout index and funds, reject duplicate payout allocation, source/scope/account/timestamp corruption, missing/extra journal and missing fund |
| Causal authorized P23 invoice → P24 capture → P25 payout acceptance (one flow) | BillingLedgerBridge + AuthorizedCaptureRegistry + AuthorizedPayoutRegistry | checked causal reconstruction (no new record kind) | Original P23/P24/P25 source fact and trusted zero-journal genesis; apply original invoice, capture and payout transitions in causal order on private Ledger; require first-time Committed and compare all three journal entries and involved account balances against supplied exact reference Ledger | 3 tests: original registry index and journal parity, missing/extra previous ledger state, tampered payout/capture source, bad billing accounts |
| P25 original capture-to-payout evidence and journal parity | cofi-payout-authorization source plus PayoutLedgerBridge | authorized.payout_evidence v1 | Decode checked P24 capture, validate original payout ID, payment ID, bank ref, gross/fee/net, scope/currency and payout accounts; run original P05/P25 bridges on private Ledger clone, require Replayed | 3 focused tests: accepted P24+P25 journal parity, changed amount/fee/bank/source/times/business, missing journal, schema/ancestry corruption |
| Checked P24 capture source evidence and exact P05 journal parity | cofi-payment-authorization source + original PaymentLedgerBridge | authorized.capture_evidence v1 | Rebuild checked original P23, preserve capture event/payment/connector IDs, times, and original receivable/processor-clearing accounts; run original bridge on a private Ledger clone and require Replayed | Three tests: accepted original P23+P24 journal, bad source/account/timestamp and changed replay, invalid schema/extra fields/1MiB |
| Original revenue distribution and immutable split rule/journal parity | `cofi-community::RevenueDistributionBridge` | `community.distribution` v1 | Decode checked original distribution event and typed embedded rate-split rule; run original bridge on private Ledger clone and require original `Replayed`; no real ledger mutation | Three focused tests cover roundtrip, original posting parity, changed rule version/BPS, amount, scope, currency, times, source and business identity, missing ancestor, invalid data and 1 MiB boundary |
| Original community fund allocation/transfer source fact and journal parity | `cofi-community::FundAllocationBridge` / `FundTransferBridge` | `community.fund_movement` v1 | Rehydrate checked exact event/source-account DTO, then invoke original bridge on a private cloned independently recovered `Ledger`; require `Replayed`, reject `Committed` and altered accepted source/journal; no real Ledger mutation | Three tests: accepted allocation+transfer equality, duplicate/conflicting business identity, altered source IDs/times/accounts/amount/scope/currency, missing original journal and malformed input |
| Original authorized governance fund spend journal parity | `cofi-spending::ApprovedFundSpendEvent` + `FundSpendBridge::verify_committed` | `governance.fund_spend` v1 | Decode checked source/spend/proposal/org/community/fund, exact amount/purpose/currency/account/timestamps; reconstruct original policy, proposal and approval quorum engine first; verify only the existing original immutable posted journal through read-only original bridge | Three tests: accepted quorum and exact preexisting journal, missing vote/history and changed source/amount/scope/account/times, strict schema and conflicting business/proposal/source identity |
| Original governance spending approval vote and quorum state | `cofi-governance::SpendingApproval` + `GovernanceEngine::approve` | `governance.approval` v1 | Decode checked source/approval/proposal/approver/timestamp, reconstruct policy and proposal history, apply only original GovernanceEngine::approve; derive original Pending/Approved transitions, immutable approver IDs and ApprovedSpendingAuthorization at quorum | Three tests: reference equality of pending/quorum authorization, duplicate ID/person/suspended/ineligible approver and missing policy/proposal/early or extra vote, strict record and 1MiB boundary |
| Immutable governance spending proposal submission facts | `cofi-governance::SpendingProposal` + original `GovernanceEngine::submit_proposal` | `governance.proposal` v1 | Rehydrate checked source event/proposal/policy/version/requester/org/community/fund/currency/amount/purpose/created/expiry; first rebuild policy history, then original GovernanceEngine validates source, active requester membership, amount/policy and owner indexes | Three tests: checked roundtrip, duplicate idempotence and status, missing policy/community, changed same-source business identity, invalid requester/amount/expiry/scope/version and JSON |
| Immutable governance approval policy registrations | `cofi-governance::SpendingApprovalPolicy` + `GovernanceEngine` | `governance.policy` v1 | Decode checked ID/version/org/community/fund/currency/amount/quorum/eligible-role snapshot; register only through original GovernanceEngine with independently reconstructed CommunityRegistry to validate fund boundary and monotonic versions | Three tests: canonical policy and original proposal validation, idempotent duplicate versus changed same policy/version, nonmonotonic policy, missing community/fund/scope, invalid role/version/quorum/cap and malformed JSON |
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
| `cofi-community` remaining economic/authorization state | Allocation/transfer source facts now have bounded original-ledger parity verification only; distribution journal parity, allocation/transfer authorization consumption and durable canonical order remain unqualified. |
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

### Immutable revenue split rule boundary

The `community.revenue_split_rule` v1 codec preserves the original immutable
rule ID, positive version, destination funds and exact basis points, using
CoFi's original `RevenueSplitRule::new` constructor to enforce sorted
unique destinations and a total of exactly 10,000 basis points.

This **does not** prove that every destination fund exists or is authorized,
that an accepted distribution referenced this exact rule version, or that
its original journal postings can be safely replayed. Fund allocation,
transfer, distribution execution, immutable authority consumption and
journal/economic replay remain unqualified under Issue #40 and G001.

### Fund movement source-to-journal parity (bounded)

The community.fund_movement v1 codec preserves original accepted allocation
and transfer source event IDs, scope/fund identities, immutable business keys,
currency, positive amount, original effective/observed timestamps, and
for allocation the original source-cash Ledger account. Minimal public
readers were added to the two original domain event types for the source
event identity and timestamps; the domain bridges and posting algorithms
remain unchanged.

After the independently recovered original Ledger and CommunityRegistry
have been supplied, the verifier runs the **original**
FundAllocationBridge::apply or FundTransferBridge::apply on a **private
clone of Ledger** and requires the exact Replayed result. If that bridge
would commit an absent entry, or the original source facts conflict with
journal identity/postings/metadata/times, verification fails. The original
Ledger is never mutated, and no network/provider action is invoked.
Same business IDs with changed source records fail closed; exact repeated
source snapshots are idempotent.

**Not proved:** the caller's canonical Ledger and source stream are
complete, trustworthy, independently authenticated, tenant-bound,
properly ordered or transactionally accepted; neither allocation nor
transfer authorization decision/consumed grant lineage is recovered
here. RevenueDistributionEvent and its journal/posting parity are not
yet covered. An internally consistent forged Ledger plus matching
source can still satisfy local parity; G002-G008 and provenance issues
remain hard gates. No live fund movement is authorized by this adapter.



### Immutable revenue distribution source-to-journal parity

The community.distribution v1 acceptance record stores original source
event ID, tenant scope, source fund and distribution business ID, original
immutable rule ID/version, currency, exact decimal i128 amount and
effective/observed timestamps, plus the original typed versioned revenue
split rule evidence. Parsing rejects duplicate and unknown nested fields,
fractional amounts, malformed identifiers, mismatched rule versions and
nonpositive amounts. The original RevenueSplitRule codec qualifies the
10,000-basis-point split and unique canonical destination set.

On a private Ledger clone, the verifier invokes ONLY
RevenueDistributionBridge::apply and requires its exact Replayed outcome:
every journal posting (including largest-remainder allocations), original
rule, identifiers, metadata and timestamps must match the independently
rehydrated original Ledger. No caller Ledger mutation occurs; missing
journals that would be newly Committed fail. Same distribution business key
and source-event identity conflicts fail closed.

This is a local consistency proof **against untrusted caller-supplied
source and Ledger**. It does NOT prove external authenticity, history
completeness/ordering, tenant-scoped atomic acceptance, destination
authorization/consumed mandate or durable source provenance. The same
limits apply to allocation and transfer parity. Issues #25, #31, #43,
G001 #22 and G002-G008 remain OPEN; do not enable money movement.


### P24 checked capture evidence is not authorization replay

The authorized.capture_evidence v1 record binds checked P23 source history to
the original P24 capture source, IDs, timestamps, account pair and canonical
payment journal ID. The original PaymentLedgerBridge::apply is invoked only
on a PRIVATE clone of the supplied independently reconstructed original
Ledger. Only an identical preexisting Replayed journal is accepted; no money
is posted to the original Ledger.

**Important:** P24 capture authorization registry is NOT recovered. The
canonical AuthorizedCaptureRegistry::capture method requires first-time
Committed journal acceptance in a causal pre-capture Ledger and cannot
be reconstructed by importing DTOs or calling capture on a fully hydrated
Ledger. This bounded codec proves local consistency only; external source
authenticity, stream completeness, tenant-scope atomic durability and
full authorization/consumed-key lineage remain unqualified.

G001 #22, authorization #43, source trust #25/#31 and G002-G008 remain
OPEN. No payment, payout or bank endpoint is enabled.

### P25 capture-to-payout evidence boundary

The authorized.payout_evidence v1 codec captures original payout source IDs,
bank reference, exact gross/fee/net amounts, scope/currency, original
timestamps and payout account bindings alongside complete checked P24
capture evidence. The original PaymentLedgerBridge and PayoutLedgerBridge
are evaluated on a private Ledger clone and require exact Replayed journal
parity. Missing original payment/payout history is rejected and no actual
money is posted.

This is NOT AuthorizedPayoutRegistry replay; original first-time
Committed authorization creation must be reconstructed from causal history
on a historical Ledger. The caller supplied Ledger and source stream are
not independently authenticated or proven complete. Consumed mandates,
tenant durable atomicity, source cutoff, G001 #22, #43, #25, #31 and
G002-G008 remain open.

### Bounded causal P23 → P24 → P25 original authorization reconstruction

The causal_capture_payout module reconstructs ONE complete P23 invoice,
P24 AuthorizedCapture and P25 AuthorizedPayout flow using original
BillingLedgerBridge, AuthorizedCaptureRegistry and AuthorizedPayoutRegistry
constructors on a PRIVATE, externally supplied zero-journal genesis Ledger.
The original first-time Committed transitions rebuild real authorization maps.
It then requires exactly three matching canonical journal entries and
matching involved account definitions and balances in an independently
supplied reference Ledger. No live Ledger is ever changed.

This is an inspectable restricted integration proof, **not a global G001
acceptance certificate**: the trustworthiness/completeness of the genesis
and final reference Ledger is not independently established; any additional
historical invoices, payments, refunds, multiple payouts, unrelated
journals, other tenants or provider events are explicitly unsupported.
Original consumed payout-to-allocation and allocation-to-transfer lineage
remains unqualified; G001 #22, #43, #25/#31, G002-G008 remain OPEN.

### Bounded P26 causal fund allocation from original reconstructed payout

The causal_allocation module requires account-only genesis, checked P25 and
P26 source, original CommunityRegistry, and separately supplied canonical
after-P25 and after-P26 reference Ledgers. It reconstructs original P23
invoice, original AuthorizedCapture and AuthorizedPayout, then constructs
AuthorizedFundAllocationRequest using the actual reconstructed payout
authorization. Only the original domain registry's first-time Created
allocation is accepted. It must produce the exact fourth canonical journal,
preserve fund and bank cash balances and rebuild the payout consumption index.
The original ledger is never modified; this is private historical replay.

This proves exactly one accepted full payout-to-allocation chain ONLY.
The supplied source chronology, tenant/fund registration and intermediate
and final Ledgers are not externally authenticated. Multiple simultaneous
payments/allocations, authorized fund transfers, source completeness/cutoff
and durable tenant-atomic persistence are not qualified. G001 #22, #43,
source-trust #25/#31 and G002-G008 remain OPEN.

### Bounded causal P27 transfer authorization from original allocation

The causal_transfer module reconstructs exactly one P23→P27 accepted chain.
It uses real previously accepted domain authorizations P24, P25 and P26
constructed from original transitions, then creates an original P27
AuthorizedFundTransfer using the actual P26 AuthorizedFundAllocation.
The original FundTransferBridge must commit a new historical journal,
and the P27 registry's allocation consumption index must be rebuilt.
The resulting five journal objects, relevant account definitions and
balances must equal the externally supplied after-P27 historical Ledger.
All writes are to a private local historical Ledger, never actual funds.

This single-flow result is not independent authentication of any event
stream, tenant, community registration, genesis or reference snapshot.
Multiple accepted financial flows, refunds, external bank/provenance
evidence and G002 tenant-atomic persistent history remain outside scope.
G001 #22/#43, audit/reconciliation #31, source trust #25 and G002-G008
remain OPEN; no production authorization is implied.

### Ordered two-tenant causal P23/P24/P25 acceptance slice

`causal_capture_payout_stream` accepts a **nonempty contiguous**
1-based sequence of complete P25 source receipts, each containing checked
P24/P23 ancestry, with independently supplied billing account bindings.
It reconstructs the ORIGINAL shared AuthorizedCaptureRegistry and
AuthorizedPayoutRegistry on a PRIVATE zero-journal Ledger for several
complete, **noninterleaved** invoice/capture/payout flows. Each original
billing/capture/payout call must newly Committed/Created, so consumed payment
and payout keys remain shared across the entire supplied stream; changing
or reusing an accepted source is refused. It compares all expected exact
journals, touched account definitions and running balances with a separately
supplied final Ledger and rejects missing/extra journals. No live Ledger,
provider, payment network or bank is touched.

Focused tests cover two different organizations with six original journals,
both real original authorization registries with two accepted authorizations,
duplicate/gapped/reordered stream positions, repeated consumed payment,
incomplete reference journal list, corrupted source and cross-tenant account
mixing.

**Scope / limitations:** caller-supplied sequence numbers do NOT prove an
externally authoritative stream order or cutoff. This does not replay
interleaved arbitrary historical events, multiple partial captures/payouts,
payout-to-allocation-to-transfer consumption across **all** flows, or all
registered/unreferenced accounts. The provided genesis and final Ledger are
NOT cryptographically authenticated, nor is source completeness or
transactional tenant durability established. This is local consistency
only, not complete G001 or G002 production qualification. #55, #43,
#22, #25, #31 and G002-G008 stay OPEN.


### Staged two-tenant original P23–P27 authority reconstruction

`causal_fund_stream` extends the bounded shared payment stream by
rebuilding original AuthorizedFundAllocationRegistry and
AuthorizedFundTransferRegistry with the genuine domain apply methods on
a PRIVATE historical Ledger. It accepts complete typed P25/P26/P27 sources
for multiple organizations, in the **explicitly staged order**: all P23–P25
flows, all P26 allocations, then all P27 transfers. No private authority
maps or substitute balance/transfer algorithms are imported.

All canonical initial accepts must be Committed/Created and the same
original registries track consumed payout and allocation keys across
the entire supplied stream. After each stage, 3×N, 4×N and 5×N accepted
journal entries must match the corresponding independently supplied
reference, including exact postings/metadata, all involved original
accounts and balances. The untouched genesis and source references are
caller supplied, not independently proven authentic. Corruption, wrong
tenant/fund, consumed authority reuse, reordered/incomplete evidence,
missing reference stages and changed times fail closed in focused tests.

**Strict nonqualification:** this is NOT arbitrary interleaved tenant
history, partial capture/payout, refund/distribution/proprietary provider
ordering, complete source cutoff, independent signature/attestation or
durable atomic tenant storage. It cannot prove external completeness of unrelated
accounts/financial facts or protect against coherently forged source +
reference Ledger. Parent G001 #22, #55, #43, #25/#31 and G002–G008 remain
OPEN; no live money, bank or provider dispatch is authorized.


### Entire Ledger account inventory at the P25/P26/P27 checkpoints

The bounded multi-flow replay adapters now require **read-only full account
inventory and balance parity** between the supplied account-only genesis,
the privately reconstructed historical Ledger, and each supplied reference
checkpoint. The read-only Ledger::accounts() method exposes the entire
deterministic account inventory without permitting unchecked mutation.
Account additions, removals, definition changes and changed balances are
rejected even for accounts not referenced by P23-P27 financial journals.

Two test-first regressions initially FAILED against the former touched-only
checks (an extra P25 reference account and extra genesis/P25/P26/P27 account
registrations). Both pass after the full inventory guard. Tests exercise
all four failure cases on the shared two-tenant original-domain stream.
No persistence DTOs directly populate a Ledger or authorization index.

This strengthens **internal parity only**. It does not independently
authenticate the genesis or reference Ledgers, establish an externally
complete account list, prove source stream order/cutoff, or authorize
account registrations between checkpoints. Arbitrary event interleaving,
unrelated economic facts, G001 #22 and G002-G008 remain unqualified.
A forged coherent genesis/reference pair is still possible.

### Versioned governance approval policy facts

The governance.policy v1 record rehydrates the ORIGINAL immutable SpendingApprovalPolicy
through its checked constructor, then registers it only via GovernanceEngine::register_policy
against the independently reconstructed CommunityRegistry. This rebuilds policy
versions and their original community/fund/organization/currency constraints.
Byte-equivalent repeated registration facts are idempotent in the codec wrapper;
different contents under the same policy identity/version or nonmonotonic
versions are refused. Eligible roles use checked original enum variants,
original role sorting and strict integer amounts.

**A registered policy is not an approval**: SpendingProposal, SpendingApproval,
quorum votes, original ApprovedSpendingAuthorization, consumed spend authority,
and FundSpendBridge journals are not persisted or rehydrated by this slice.
It cannot establish an externally authentic source stream, cutoff, tenant binding
or G002 atomic durability. G001 parent #22 and G002–G008 remain open.

### Governance proposal facts without approval state

The governance.proposal v1 record preserves the original proposal source-event
identity, original policy ID/version, requester and organization/community/fund,
exact decimal amount, purpose, creation and expiration timestamps. Only the
original SpendingProposal constructor and GovernanceEngine::submit_proposal
rebuild accepted proposal IDs/source events and Pending status after checked
versioned governance.policy history and CommunityRegistry are restored.
Any changing same source event, missing original policy, inactive requester,
wrong scope or over-limit amount fails through the original domain methods.

**No vote, approval or spend has been granted**: the approval source-event
stream, quorum, approved spending authority, consumed mandate, and
FundSpendBridge posting remain separate G001 grains (#64). Historical
source completeness/tenant-bound accepted ordering, G002 durability, and
independent authenticated genesis are unproven. Parent G001 #22, #25/#31,
and G002-G008 remain open.

### Original spending approval votes and quorum

The governance.approval v1 source record contains the accepted vote source
event, approval identity, proposal identity, approver party and exact i64 time.
Original policies and proposals are first replayed through the checked
governance.policy/governance.proposal codecs and their original engine methods.
Every vote then enters ONLY GovernanceEngine::approve; this original code
enforces per-party uniqueness, active membership and eligible role, valid
proposal ancestry, expiration/time eligibility and the approved quorum.
ApprovedSpendingAuthorization, approver identities and approved_at timestamp
are DERIVED, never read directly from unchecked persistence JSON.

A completed governance quorum is **not a spend execution**. The separate
cofi-spending ApprovedFundSpendEvent and FundSpendBridge accepted journal
and consumed authority require independent source proof, G001 coverage and
all externally trusted tenant/source sequence checks. #66, #64, #22,
source #25/#31 and G002-G008 remain independently open.

### Bounded governed fund spend source-to-journal parity

The governance.fund_spend v1 codec retains accepted source-event identity,
business spend/proposal keys, scope, community, fund, currency, exact amount,
purpose, expense account and effective/observed timestamps. It rehydrates
the original GovernanceEngine only through policies, accepted proposals and
ordered approval votes before invoking FundSpendBridge::verify_committed.
No Apply/Commit endpoint is called, and no Ledger mutation occurs.

Changed source or spend ID, reused consumed proposal, missing quorum, wrong
expense kind/tenant/currency, altered amount, purpose or posting metadata
fail the original domain check. An existing exact journal is necessary.

External caller-provided history and Ledger may still be forged together;
neither quorum nor journal parity alone proves global accepted event order,
authorization source completeness, immutable consumption across all tenants,
transactional persistence or external funds. Parent G001 #22, #64/#66/#68,
#25/#31, G002-G008 remain OPEN. Production spending remains blocked.

### Strict single-pass governance acceptance order (Issue #70)

The `governance_history` checked adapter consumes one **caller-supplied** stream of
versioned original policy, proposal, approval and fund-spend records in their
asserted acceptance order. It invokes only the original GovernanceEngine
registrations, submissions and approval decisions; for spending it calls the
original read-only FundSpendBridge::verify_committed against the supplied
preexisting Ledger. No domain algorithm or accounting journal is duplicated
or mutated. Duplicate accepted events, proposals before their policy,
approvals before their proposal, spending before quorum, duplicate consumed
proposal authority and missing/changed committed journals fail closed.

This does **not** establish independent chronological authenticity. A caller
can forge, omit or reorder source records and a coherent reference Ledger;
no external source witness, trusted tenant sequence/cutoff, durable consumed
mandate, migration, signature or provider effect is provided. Existing grouped
codecs retain their documented bounded semantics. Parent #22, source trust
#25/#31 and G002-G008 remain OPEN. Production money remains disabled.

### Original disbursement-creation source and spend-bound replay (Issue #72)

The `disbursement.creation` v1 codec retains the original immutable creation
source-event/disbursement IDs, beneficiary/destination references and exact i64
creation timestamp. It reconstructs only the original DisbursementCreation
constructor. On a separately supplied original governance engine, community
registry and preexisting Ledger, the bounded replay adapter invokes only
original DisbursementEngine::create with each checked original committed fund
spend source. It restores the original Ready state, spend-to-disbursement
consumption index and idempotent source retry semantics; changed accepted
source IDs, second disbursements for one spend, creation before execution and
missing/mismatched original journals fail closed. Ledger remains read-only.

**Not qualified:** original submission/terminal lifecycle, independently
verified provider origin/signature, complete accepted event/observation
history, cross-tenant durable consumption and sequence, authenticated
reference Ledger, G002 atomicity or live transfers. Issue #72 is a bounded
partial codec only. Parent G001 #22, provenance #25/#31, and G002-G008 stay
OPEN. No real provider network activity or economic posting is enabled.

### Original disbursement submission and terminal lifecycle facts (Issue #74)

The `disbursement.submission` and `disbursement.terminal` v1 records retain
original accepted disbursement/source/provider references, exact i64 event
timestamps and typed Settled/Failed settlement or failure identities. Small
read-only getters expose original immutable source fields; the codecs use
only original constructors, reject unknown/duplicate JSON fields and oversized
records, and never hydrate original domain maps directly.

The bounded `replay_disbursement_lifecycle` consumes a **caller-supplied**
interleaved creation → submission → terminal stream, invokes only original
DisbursementEngine::create/submit/record_terminal against checked original
spend and immutable Ledger, and rebuilds original Ready/Submitted/Settled/
Failed state plus original consumed provider reference indexes. Original domain
checks reject impossible order/times, changed source IDs and replay conflict.

**No trusted provider observation or live settlement is proven**: these are
internally consistent caller-supplied lifecycle facts, not authenticated
webhooks or bank evidence. Full tenant admission/sequence/cutoff, source
completeness, durable transaction boundary, actual external attempt and
provider verification remain unqualified. G001 #22, provenance #25/#31,
G002-G008 and all production financial gates remain OPEN.

### Accepted lifecycle source uniqueness (Issue #77)

The original `DisbursementEngine` intentionally returns `Replayed` for exact
command retries. An ordered **accepted historical fact stream** must not
reinterpret those retries as additional accepted transitions. The bounded
`replay_disbursement_lifecycle` therefore accepts only first-time original
`Created`, `Submitted`, `Settled` or `Failed` results and rejects `Replayed`
for creation, submission and terminal facts. The regression uses three
identical duplicate accepted-source events and verifies a complete unique
Ready -> Submitted -> Settled stream remains accepted. No original domain
engine behavior, financial posting or provider execution was changed.

This guards against duplicated entries **inside caller-supplied history**; it
does not establish tenant/source authentication, an external history cutoff,
provider-signed receipts, completeness, or G002 transactional persistence.
Parent #22 and G002-G008 remain open.

### Shared governance-to-disbursement acceptance order (Issue #79)

The `financial_history` bounded adapter consumes one caller-supplied,
interleaved accepted Policy, Proposal, Approval, FundSpend, Creation,
Submission and Terminal stream. Original `GovernanceEngine`,
`FundSpendBridge::verify_committed` and `DisbursementEngine` alone
rebuild state. A creation references an **already encountered** verified
original fund-spend source event ID, not an arbitrary accepted source supplied
later or a separately prebuilt governance engine. Exact duplicate accepted
policy, proposal, vote, spend, creation, submission and terminal facts are
rejected, even when a domain command's retry would be idempotent.

This adapter does **not** authenticate the external record source or its
completeness/tenant-global chronology and cutoff; it reads an independently
supplied immutable reference Ledger but cannot prove its real-world origin.
No PostgreSQL storage, transaction, original Ledger mutation, provider
observation, money movement or release enablement is introduced. Parent
G001 #22 and G002-G008 remain OPEN.

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
