# First implementation grains

These are bounded proposals for P0, not code delivered by this task. They preserve existing pure domains and cover **all** financial/authority registries, not just journal rows. Proposed paths are future paths and must be checked against the live tree before starting. Prefer one reviewable PR per grain; split further if proof scope grows. No provider network call or live-money enablement is in these grains.

## G001 — Validated persistence codec and replay contract

Objective: serialize accepted facts without making private fields or unchecked DTOs canonical. Scope: versioned record envelopes, exact numeric/date/ID codecs and replay adapters using existing constructors. Proposed paths: `crates/cofi-storage/src/codec/`, `crates/cofi-application/src/replay/`, domain tests only where a minimal checked interface is necessary. Dependency: adopted plan and inspected current domain API.

Tests: i128 posting/u128 balance extremes, invalid/fractional/overflow numbers, source order/metadata/timestamp preserved, exact replay/result equality for every current domain transition, changed duplicate/business key conflicts, unrecognized codec fails closed. Risk: serialization bypasses constructor invariants. Acceptance: a complete registry inventory maps each accepted fact/result and rebuild dependency; roundtrip plus independently replayed state matches in-memory reference. No new money algorithm.

## G002 — PostgreSQL schema, scope and migrations

Objective: one auditable durable schema foundation. Scope: migration runner/checksums, aggregate/scope lock rows, canonical facts/journal/postings/accounts, registry state/version tables, replay identities, authority/reservations, audit, inbox/outbox/jobs, with composite scope foreign keys and numeric bounds. Proposed paths: `crates/cofi-storage/migrations/`, `src/schema.rs`, integration test support. Dependency: G001 record/schema inventory.

Tests: real PostgreSQL constraints, runtime/admin roles, RLS defense with explicitly scoped application sessions, cross-tenant/environment foreign key rejection, connection pool context reset, migration interrupted/restarted and poison/checksum mismatch. Numeric storage rejects values outside domain range before mutation. Risk: RLS assumed sufficient under superuser/owner or session state leakage. Acceptance: fresh database and upgrade from previous migration reproduce schema; runtime cannot bypass scope or mutate protected history. P0 system principals are explicit, not implicit global authority.

## G003 — Deterministic hydration and read parity

Objective: recover the same existing economic/authority state from durable accepted facts. Scope: ordered account/registry/entry reconstruction, versioned result replay, consistency report; initially bounded complete scope replay, no clever selective SQL accounting. Proposed paths: `cofi-storage/src/read/`, `cofi-application/src/hydrate/`. Dependency: G001-G002.

Tests: every existing domain fixture serialized/reloaded by a new process; corrupted fact/hash/version, missing dependency, audit chain break and scope mismatch produce explicit quarantine; balance recomputation equality. Risk: journal rebuild works while proposal/subscription/business identities are lost. Acceptance: all registry/replay/authority objects represented, hydration inventory complete, restart produces exact reference state. Document performance limits; optimization follows evidence.

## G004 — Serializable unit of work and concurrency

Objective: atomic read/check/domain-evaluate/write without lost updates. Scope: one transaction wrapper, stable-order scope/aggregate locks, SERIALIZABLE retries only for database-local work, fixed logical command time/IDs, pure-domain invocation and staged persistence. Proposed paths: `cofi-storage/src/transaction.rs`, `cofi-application/src/unit_of_work.rs`. Dependency: G003.

Tests: two independent processes racing balances, budgets and business keys; serialization/deadlock retry; inject failure after each staged statement; no network in retry closure; committed registry+ledger+audit+result+outbox all present or all absent. Risk: a retry invokes an external effect or uses new IDs/time. Acceptance: multi-process conformance preserves current invariants and atomic all-registry state, not merely tests with one mutex/mock store.

## G005 — Durable economic replay and uncertain COMMIT

Objective: exact replay after response/process loss, including a lost database commit acknowledgment. Scope: durable normalized request/result and immutable economic keys/tombstones; uncertain commit resolution on primary by stable operation identity; prevent ambiguous attempts from releasing authority. Proposed paths: `cofi-application/src/commands/`, `cofi-storage/src/idempotency.rs`. Dependency: G004; this hardens the core replay acceptance path before exposing an API.

Tests: kill process after commit before acknowledgment; lose connection during COMMIT; repeated same key returns original result, changed facts conflict; expired response cache cannot free economic identity; concurrent exact requests have one effect; unavailable primary yields uncertainty, not presumed rollback. Risk: domain, HTTP, provider and event dedup keys conflated. Acceptance: exact replay survives restarts and ambiguous commits; retention policy preserves economic uniqueness.

## G006 — Durable work/inbox/outbox and dispatch protocol

Objective: notifications and external commands have different safe delivery semantics before any live connector exists. Scope: PostgreSQL leases/fencing, notification retries/dead letters, verified-inbox acceptance interface, queued/claimed/DISPATCHING/UNKNOWN command storage and deterministic fake executor. Proposed paths: `cofi-service/src/worker/`, `cofi-storage/src/work/`, `cofi-application/src/observations/`. Dependency: G005; auth scope/reservation checks reuse the existing authority boundary.

Tests: duplicate/out-of-order fake observations, crash before/after durable dispatch marker, stale worker/reclaimed lease, revoked grant between queue/dispatch, same event notification replay, no second payment dispatch from UNKNOWN. Concurrent dispatch workers use the same attempt identity and cannot invent a replacement. Risk: copy generic at-least-once queue logic into payments. Acceptance: money attempts stop at evidence-bound ambiguity; notification retries remain safe and canonical events only derive from committed state. Real signature/provider validation arrives in P2/P3; unverified live webhook ingestion is prohibited.

## G007 — Verification, backup/restore and repair plans

Objective: prove recoverability of all economic/authority/attempt state. Scope: read-only doctor/recompute, backup/WAL/PITR runbook, restore quarantine, projection rebuild into shadow tables, approved append-only correction plan format. Proposed paths: `cofi-service/src/doctor/`, `cofi-storage/src/recovery/`, `docs/operations/`. Dependency: G006.

Tests: fresh-instance restore with missing/old observations and in-flight DISPATCHING attempts, replay identities survive, restored live dispatch disabled pending provider reconciliation; corruption detection, safe projection swap, no journal rewrite, key recovery and witness comparison. Risk: restore duplicates a payment sent after the backup or treats a stale grant as live. Acceptance: timed restoration with verified invariants and documented unresolved external exposure; no claim that backup alone proves external money continuity.

## G008 — P0 conformance and operational qualification

Objective: qualify the complete storage boundary on an exact CoFi head. Scope: reusable real-PostgreSQL crash/concurrency/replay suite, scope/role/codec/migration evidence, basic health/lag/case telemetry, documented development startup and resource measurements. Proposed paths: `cofi-storage/tests/`, `cofi-application/tests/`, existing CI additions with pinned database/runtime dependencies, `docs/operations/qualification.md`. Dependency: G001-G007.

Tests: repeat the acceptance matrix in [durable ledger plan](../architecture/DURABLE_LEDGER_PLAN.md), retained existing Rust tests/fmt/Clippy/MSRV, repository identity and actual exact-diff reviews. Measure representative scope replay/resource behavior; choose operational bounds from results, not invented throughput numbers. Risk: green unit tests mistaken for production readiness. Acceptance: all required evidence is inspectable, owner/recovery procedure defined, current head qualified. P0 completion authorizes P1 service/sandbox work, not real payment activation.

## Review protocol per grain

Before coding: verify main/open work, freeze objective/dependencies/invariants/exclusions, record source rights/closure if any. During work: small normal commits, preserve deterministic tests and source ownership. Before merge: diff scope, required commands, identity, Alibaba OpenCodeReview and genuine TypeSafe Jev against exact base/head; a later commit requires requalification. Merge normally only when required evidence exists. A workflow that merely prepares an OCR delegation does not provide semantic review. Never manufacture review findings or substitute self-report for a missing credential/run.

P1 follow-on grains start with authenticated scope/key lifecycle and wire contract, then deterministic sandbox/SDK/CLI/desktop client. P2 ingestion starts read-only. Hyperswitch import begins only in a bounded connector experiment after storage/service boundaries, not as an unrelated giant source import.
