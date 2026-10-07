# Durable ledger plan

P0 is the highest-priority foundation. The existing `Ledger` has BTreeMap accounts, entries, replay indexes and balances. It has valuable checked, fail-before-mutation semantics; preserve them. Durability must extend to commercial/authorization antecedents, not just journal rows.

## Acceptance and failure contract

A production command is accepted only after a committed PostgreSQL transaction. An acknowledgement contains stable command ID, scope sequence and result. A lost acknowledgement is ambiguous to the client: retry using the same logical identity, or read the command. The database is not an in-memory cache flushed later. `synchronous_commit` stays on; primary failover durability and replica acknowledgement policy must match the promised recovery point.

The atomic unit is: authenticated command identity + validated antecedents + authorization/budget change + economic effect + journal/postings + replay indexes + audit record + canonical result + event outbox. An external-command reservation can commit without an economic posting, but must be labeled pending and cannot claim provider success. Failed domain validation leaves no successful financial identity, consistent with current tests.

## Schema strategy

Identifiers below are design coordinates, not executable migrations. Use typed SQL columns for identity, money, currency, status, lineage and sequence. JSON is reserved for bounded immutable versioned payloads, not unconstrained authoritative facts.

| Table family | Identity and constraints | Purpose |
| --- | --- | --- |
| organizations/projects/environments | Composite ancestry references; live/sandbox explicit | Authenticated ownership |
| ledger_scopes | `(organization_id, environment_id, scope_id)` primary key; monotonic `revision` | Serialization point, schema/codec revision and audit tip |
| accounts | Scope tuple + account ID; immutable currency/kind after use | Registry; no cross-scope join |
| journal_entries | Scope tuple + entry ID; unique scope sequence; effective/recorded timestamps, immutable payload/hash/codec | Canonical accounting history |
| postings | Entry tuple + ordinal; FK to same-scope account/currency; closed side enum; positive amount | Balanced movement, original posting order preserved |
| account_balances | Scope/account tuple, unsigned debit/credit totals, projection sequence | Transactional projection, independently rebuildable |
| operation_results | Scope + operation namespace + idempotency key; canonical request digest, retained normalized request, result and status | Restart-safe replay, client response recovery |
| economic_effect_keys | Scope + effect kind + business ID (+ component ID when multiple valid effects exist) | Prevent capture/refund/allocation duplicates through different API identities |
| economic_objects and transitions | Scope + typed object ID/version; immutable transition sequence and ancestry refs | Billing/payment/fund/governance facts |
| grants/approval_receipts/reservations | Scope-bound policy/grant IDs, versions, limits, consumed/reserved counters | Exact intent authority; concurrent spend prevention |
| external_commands/attempts | Scope + command ID; immutable provider account, flow, payload digest, idempotency reference; state, fencing epoch | Durable execution intent; distinct from economic posting |
| provider_inbox/observations | Tenant/environment/provider account/event ID, raw-body digest, verified normalized observation and verifier version | Webhook deduplication and replayable evidence |
| event_outbox/deliveries | Scope/event ID, aggregate sequence, payload version, destination version, delivery attempt | At-least-once notification; no financial re-execution |
| reconciliation_cases | Evidence refs, cutoff, classifications, disposition and authorized adjustment refs | Break management without rewriting history |
| audit_events/checkpoints | Scope/stream/sequence; previous digest, canonical digest, actor/causation | Tamper evidence, external witnessing |
| migration_registry/import_manifests | Immutable migration checksum, data codec revision, source/run IDs, external-ID maps | Safe upgrades and non-duplicating import |

All multi-table foreign keys carry organization and environment, and ledger scope when financial. Primary keys never rely on a globally unique opaque ID as the isolation boundary. Provider event uniqueness includes connected/provider account: two merchants can legitimately receive the same provider-native ID namespace. A reused event ID with different bytes is quarantined, not overwritten.

## Numeric representation

PostgreSQL `BIGINT` cannot preserve CoFi's i128/u128 range. Use exact `NUMERIC` columns with explicit integer and range CHECK constraints, plus validated integer driver conversion. A `NUMERIC(39,0)` typmod silently rounds fractional input before a CHECK can inspect it; do not rely on that typmod to reject fractions. An unconstrained NUMERIC plus `value = trunc(value)` and finite/bounds checks rejects fractional values without rounding. Reject NaN/infinities explicitly. Posting inputs must be `1..170141183460469231731687303715884105727` (i128 maximum). Debit/credit cumulative balances can reach `340282366920938463463374607431768211455` (u128 maximum). Reject fractional scale, negatives, overflow and scientific-notation ambiguity at wire boundaries. Use decimal strings consistently for money and quantity in JSON.

The exact SQL type alone permits values outside u128; the bounds are mandatory. Retain separate debit and credit totals so signed net arithmetic cannot overflow or change current semantics. Different currency exponents come from a versioned registry; provider-specific exponent differences are explicit adapter conversions. FX requires a separately approved quote, fees, source/destination amounts and balanced currency-specific postings; no implicit conversion or cross-currency netting.

## Domain integration without a rewrite

First implement validated storage codecs through public constructors/getters. Preserve original journal ID, posting order, timestamps, metadata, account kind/currency and exact replay equality. Hydration registers accounts and replays accepted journals in canonical scope sequence into a fresh `Ledger`. Run domain validation rather than trusting database bytes. Do not expose mutable maps or use unchecked deserialization to skip constructors.

Under a transaction, load the affected scope's canonical registry/history, rehydrate, run the existing domain/authorization operation, and persist the resulting accepted transition. This initial path is an expensive reference implementation with a published measured limit. Provide a deterministic conformance harness comparing in-memory and durable outcomes. No live operation may use a stale process-local ledger as its authoritative precondition.

Snapshots are optimizations later: immutable, versioned, hash-bound to a journal prefix and independently verified. A snapshot never replaces source history. A snapshot-plus-delta operation must match full replay exactly. Selective per-account/aggregate hydration is only allowed after identifying every dependency the domain operation reads and proving it cannot omit negative-balance, budget or authorization preconditions. Do not accidentally create a SQL-only second accounting algorithm.

## Transaction and concurrency protocol

1. Parse/authenticate outside the transaction. Resolve organization/environment from credentials. Validate syntactic bounds. Allocate proposed request IDs outside retry loops; trusted time is fixed for this command's accepted record.
2. Begin `SERIALIZABLE` transaction with bounded statement/lock timeout and set transaction-local RLS context before any scoped query. Acquire the scope row `FOR UPDATE`. For a multi-scope atomic administrative action, acquire rows in a globally sorted order; cross-organization movement normally uses separate balanced workflows.
3. Query the logical operation identity. If already committed, compare normalized intent/digest and return the stored result; changed intent is conflict. Do not include transport request ID or newly generated retry timestamp in the semantic digest.
4. Recheck active membership/grants, freeze state, immutable policy version and required canonical antecedents. Lock budget/reservation/aggregate rows in deterministic order. Rehydrate or load a verified state prefix.
5. Execute pure domain/authorization logic. Domain validation failure rolls back all economic state, reservations and success indexes. Security denial logs can be recorded separately, without claiming accepted finance.
6. Insert canonical transition, journal and postings; update balances and budget counters; insert business/replay keys, result, audit and event outbox. Advance scope revision using expected revision. Enforce currency-balanced journal checks at commit via deferred constraint trigger or a restricted commit function, in addition to domain validation.
7. Commit. Only then return success and wake workers. A connection lost during COMMIT must be resolved by operation identity on the authoritative primary; it is never treated as definitely rolled back.

`40001` serialization failures and deadlocks permit bounded jittered **database-only** retries with the same semantic input, allocated identities and logical time. Never retry a provider call in this loop. Unique violations are resolved as exact replay or conflict, not blanket "success". Persistent contention becomes an explicit retryable conflict with request/command ID; monitor it before introducing finer locks.

RLS is defense in depth, not a substitute for predicates. Application roles cannot bypass RLS; use FORCE RLS where appropriate. Transactions set/reset context safely under connection pooling. Database migrations and administration use separate offline roles. Journals and accepted transitions deny UPDATE/DELETE to runtime roles; indexes/projections may be repaired through audited tooling. The DB operator remains powerful: cryptographic external checkpoints are needed for stronger tamper evidence.

## Idempotency layers

Keep four identities distinct: API logical request, domain journal/economic business key, external provider command, and observation/delivery identity. One can be retried without re-executing the others. Scope API keys by operation namespace; scope permanent economic keys by effect kind. A capture and its refund cannot accidentally collide, nor may two captures of the same provider capture use different keys to post twice.

Economic replay keys and normalized accepted facts are retained for the financial retention period. API cache expiry must not free a committed business identity. A pending external attempt retains its provider idempotency reference and expiry/capability evidence. If a provider forgets an idempotency key, CoFi's history still blocks unsafe resend. Content hashes are indexed accelerators; retain canonical normalized bytes to verify exact equality and handle codec evolution explicitly.

## Outbox, inbox and external execution

Canonical events are inserted in the same transaction as their economic effect. Event workers claim batches using `FOR UPDATE SKIP LOCKED`, commit a lease/attempt row, deliver outside the transaction, then store acknowledgement. A crash after delivery can duplicate an event; consumers deduplicate by stable event ID. Pin event schema and signing key version per delivery. Notification retry cannot call a payment endpoint.

External commands require a different protocol: queued -> claimed -> dispatching (durably recorded before send) -> observed/unknown. Record fencing epoch and pre-dispatch authority/budget check. A crashed/reclaimed dispatching attempt is unknown, even when a process thinks it sent nothing. Another worker queries provider status; it cannot blindly redispatch. A strong provider idempotency contract can permit exact resend only within its qualified retention/window and operation/account scope. Absence in eventually consistent search is not proof of non-acceptance.

Provider webhooks are verified against raw body and qualified signature/time rules before canonical admission. Durably store receipt/evidence before returning acknowledgement, normalize under a versioned adapter, then lock the attempt and apply the state rule. Duplicates, stale ordering, mismatched amount/currency/account, and terminal contradictions cannot mutate a balance speculatively. Readback and reconciliation cases resolve contradictions.

## Recovery and consistency verification

On restart, recover committed command results; reclaim notification leases; quarantine/reconcile dispatching attempts; identify stuck reservations and scheduled jobs. Reservations for unknown external outcomes remain held or restricted until evidence resolves them. Expiring a lease is not cancelling a payment.

`cofi doctor` should support read-only checks of journal balance by currency, account existence/kind/currency/scope, monotonic sequences, replay/business-index referential integrity, debit/credit recomputation, authorization ancestry, budget consumption, observation/effect uniqueness, outbox completeness and audit tips. Offline full replay compares digest and result with materialized state. A mismatch freezes affected money writes and opens a case.

Repair rebuilds projections into shadow tables, compares, then atomically switches verified projections under maintenance authority. Never silently edit a journal to fix an imbalance. Incorrect economic accounting needs a linked reversal/adjustment with reason, prior evidence, qualified authority and its own idempotency identity. Provider settlement cannot be inferred from a locally balanced entry.

## Migrations, backups and restore

Migration files are immutable and checksum-verified. Use expand/backfill/validate/switch/contract with declared reader/writer version compatibility. Backfills are bounded, checkpointed, scope-isolated and auditable. Irreversible money-history migrations require a restore rehearsal and dual-read comparison. Avoid autogenerated destructive down migrations; rollback binary/schema compatibility separately from economic history.

Use encrypted physical base backups plus WAL archive/PITR, with separate access, retention and key-recovery procedures. Logical portable exports supplement these; they are not automatically disaster recovery. Export includes schema/codec versions, journal order, account mappings, business identities, authorizations, observations/evidence references, pending attempts, outbox/delivery state and signed digest manifest. Never export PAN/CVC or plaintext connector secrets.

Initial operator targets to validate: RPO <= 5 minutes for backup recovery and RTO <= 60 minutes for the reference deployment. They are not promises until measured drills pass; primary synchronous replication can provide stronger commit durability only under a qualified failover configuration. Track backup/WAL lag; freeze or alert according to the documented promise.

Restore into an isolated environment with **all external execution and webhook delivery disabled**. Verify keys/codecs, replay and reconciliation. Restored pending attempts may already have executed after the backup cutoff; reconcile provider readback and current settlement files before enabling writes. Do not replay old queues into live providers. Restore only notification deliveries after explicitly reviewing duplicate notifications. Financial history and provider reality determine safe reactivation, not merely a healthy DB.

## P0 qualification matrix

| Scenario | Required oracle |
| --- | --- |
| Process killed before/after every local write and COMMIT acknowledgement | Zero partial economic units; exact retry returns one committed result |
| Concurrent identical requests from separate processes | One effect, identical result; indexes and balance match |
| Same idempotency/business key with changed amount/time/lineage | Conflict, unchanged canonical state |
| Concurrent spend against one limited budget | Sum of reservations/consumption never exceeds authorized envelope |
| Cross-tenant/environment object, queue, credential and export references | Rejected by service and DB constraints |
| i128/u128 limits, large decimal wire strings, malformed/fractional input | Exact round trip or deterministic overflow rejection |
| Serialization/deadlock/connection loss during commit | Bounded safe recovery without new command identities |
| Outbox crash after send; duplicate/out-of-order webhook | Duplicate notification allowed; no duplicate posting |
| Provider accepts then socket times out or worker dies | UNKNOWN; no provider fallback/resend until evidence qualifies it |
| Corrupt snapshot/index/projection, truncated export | Detection, write freeze, safe rebuild or refusal |
| Upgrade/backfill interrupted; backup before in-flight payment restored | Resume deterministically; live dispatch remains disabled until reconciliation |

Use actual PostgreSQL and independent OS processes for durability/concurrency tests, not only mocked repositories. PostgreSQL version, storage settings, platform, dependency/MSRV and test logs are part of evidence. Exit P0 only when grains G001-G008 pass exact-head review and a restore drill. [First grains](../canonical/FIRST_IMPLEMENTATION_GRAINS.md) specify bounded delivery.
