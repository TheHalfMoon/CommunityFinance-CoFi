# CoFi financial platform master plan

Research date: 2026-10-07. Baseline: `c640844675895578ce52e67c591308e4ddb99037` on canonical `main`; release `v1.2.0`.

Status: proposed repository-owned master plan. Implementation is not started. Publication on a feature branch is not adoption into `main`; exact-head CI, Alibaba OpenCodeReview, genuine TypeSafe Jev review, and a normal merge remain required. This package does not change release support claims in PROJECT_STATUS.md.

CoFi should become the portable financial control layer for applications: one reconstructable economic history, one explicit authorization lineage, and replaceable execution providers. The initial market is developer-led SaaS/platform teams with usage revenue and controlled multi-party spending. The entry product is shadow reconciliation beside an existing processor; the first execution product combines durable billing and one provider with safe failure recovery.

## Reading order

1. [North star](COFI_NORTH_STAR.md): product thesis, alternatives, non-goals and moat.
2. [Verified live state](../research/LIVE_STATE.md) and [evidence register](../research/SOURCE_EVIDENCE.md): what exists and what was inspected.
3. [Stripe gaps](../research/STRIPE_GAP_ANALYSIS.md), [reuse matrix](../research/SOURCE_REUSE_MATRIX.md), [Hyperswitch import plan](../research/HYPERSWITCH_IMPORT_PLAN.md).
4. [Architecture](../architecture/COFI_ARCHITECTURE_2.md), [money state](../architecture/MONEY_STATE_MODEL.md), [durable ledger](../architecture/DURABLE_LEDGER_PLAN.md), [connectors](../architecture/CONNECTOR_PLATFORM.md).
5. [Developer platform](DEVELOPER_PLATFORM.md), [billing and pricing](BILLING_PRICING.md), [community finance](COMMUNITY_FINANCE_2.md), [agent finance](AGENT_FINANCE.md), [adoption](ADOPTION_STRATEGY.md).
6. [Security and operations](../architecture/SECURITY_OPERATIONS.md), [red team and scorecard](PLAN_REVIEW.md).
7. [Roadmap](PLATFORM_ROADMAP.md), [first implementation grains](FIRST_IMPLEMENTATION_GRAINS.md), [decision register](DECISIONS.md).

## Authority and interpretation

The current Rust implementation and its existing tests describe shipped behavior. These documents describe future work. New APIs, CLI commands, database tables and SDK examples are proposals, not available features. Each implementation grain must recheck live main and open work before changing code. Existing crate names stay intact; new boundaries start as modules and gain crates only when independent ownership/dependencies justify them.

Evidence strength: source implementation > qualified tests > documented contract > product claim > design hypothesis. No donor was built, benchmarked or security-qualified in this research. Root licenses are screening evidence; individual files, embedded imports and dependencies require separate qualification. The founder's stated private permission applies to `juspay/hyperswitch` only and is recorded by a stable reference, without publishing private documents.

## Consistency rules across the package

- A database commit is the only production financial acceptance boundary. In-memory success is never production acceptance.
- Provider outcomes and journal postings are distinct. Unknown acceptance freezes retry/fallback until qualified evidence resolves it.
- Existing accounting, business-key replay and authorization lineage are preserved. New infrastructure cannot accept caller-selected canonical accounting facts.
- Tenant/environment/scope is structural, including foreign keys, queues, secrets, exports and idempotency.
- No network call occurs inside a retryable database transaction.
- Recovery, migration and correction append history; analytics and snapshots are rebuildable projections.
- Licensed providers own custody, acquiring, regulated onboarding, tax filing, issuing and lending functions.

## Research limitations and remaining release gates

Public source coverage is broad but is not a proof that every line or ecosystem dependency was audited. The full accessible founder inventory was screened; selected public repositories were inspected at pinned revisions. Private repositories were not used as public evidence. Search indexing can lag live source: Gomrey's indexed revision differed from its clone, so the live clone governs the reuse record.

Some Firecrawl requests failed internally; a separate web reader retrieved remaining official product pages. Prism's Windows checkout failed on a trailing-space filename; its commit/tree and selected files were read through Git and the GitHub connector. Branch protection returned HTTP 403; rulesets returned an empty list. Neither result proves that merges are unprotected. Current Stripe availability is product-, country-, account- and release-phase-specific.

P0 grains are specified well enough to begin after this plan is adopted. Live-money release is blocked until crash/concurrency/restore tests, independently reviewed security controls, provider certification and operational readiness pass. Research does not waive those gates.
