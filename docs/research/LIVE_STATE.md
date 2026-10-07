# Verified CoFi live state

Inspection date: 2026-10-07. Canonical remote: `TheHalfMoon/CommunityFinance-CoFi`. GitHub repository metadata reports `main` as default, merge commits enabled, squash/rebase disabled, and active public repository status. Source inspected: `c640844675895578ce52e67c591308e4ddb99037`. Latest release endpoint reports `v1.2.0`, published `2026-10-07T13:53:56Z` (16:53:56 Asia/Riyadh). Release assets include Windows MSI, NSIS and SHA256SUMS.

## Code-backed foundation

The root workspace contains 21 members: ledger, community, governance, spending, disbursements, provider-contract, reconciliation, metering, audit, billing, payments, rating, invoicing, subscriptions, and seven authorization crates (rating, invoice, finalization, payment, payout, fund-allocation, fund-transfer). Desktop's Tauri Rust project is excluded from that workspace and has its own qualification workflow. Edition 2024, Rust 1.85 minimum; workspace `unsafe_code = forbid`, Clippy unwrap/expect/panic denied.

| Existing source | Verified behavior | Limit relevant to the new platform |
| --- | --- | --- |
| `crates/cofi-ledger/src/lib.rs` | Typed currency; positive i128 posting input stored as u128; balanced journal validation and explicit metadata | Currency validates three uppercase ASCII bytes, not full ISO exponent/support registry |
| `crates/cofi-ledger/src/state.rs` | Account kinds/scopes; checked debit/credit accumulation; exact journal replay; idempotency/business-key conflicts fail before mutation | Five BTreeMap fields hold accounts, entries, replay indexes and balances; no durable commit or shared-writer boundary |
| `cofi-billing`, `cofi-payments` source/tests | Receivable/revenue, capture clearing and processor payout accounting | Accounting events do not establish a real network payment executor |
| `cofi-metering`, `cofi-rating`, `cofi-invoicing`, `cofi-subscriptions` | Typed deterministic commercial state and linked authorization | RatePlan is unit-price/included-units with effective range; not a complete tiered/credit/CPQ suite |
| Seven authorization crates | Sensitive facts derived from canonical antecedents | Must persist antecedents, receipts and derived economic effects together |
| `cofi-community` allocation/transfer/distribution | Shared funds and deterministic scoped transfers | Current ledger rejects cross-scope postings; platform cross-organization movement needs explicit paired workflows |
| `cofi-governance/src/lib.rs` | Versioned policies with fund/currency/max amount, eligible roles, required approval quorum | Runtime membership changes, durable budget reservation and network execution need service qualification |
| `cofi-spending`, `cofi-disbursements` | Authorized spending and vendor disbursement lifecycle | Network acceptance ambiguity and provider persistence are future infrastructure work |
| `cofi-provider-contract/src/lib.rs` | Disbursement request derived from canonical state; observation validation and terminal conversion | Not a generic payment/acquiring/vault API; extend instead of replacing |
| `cofi-reconciliation/src/lib.rs` | Canonical versus provider disbursement discrepancy model | Not full settlement/bank/file/fee reconciliation |
| `cofi-audit/src/lib.rs` | SHA-256 chained events with attribution/timing; BTreeMap streams | In-process tamper evidence is neither durable storage nor protection against a DB administrator rewriting an entire chain |

The storage finding applies beyond ledger: commercial registries and audit/governance state also use in-memory structures. Persisting journal rows alone leaves business keys, approvals and billing antecedents vulnerable to restarts. Production acceptance must cover the whole economic unit of work.

## Desktop and public service

`apps/desktop` is Tauri 2 with React/TypeScript, packaged for Windows and localized in seven languages including Arabic RTL. Native `src-tauri/src/lib.rs` exposes only `core_manifest` and `validate_currency`; it links `cofi-ledger::Currency` directly. Its capability list is a manifest, not evidence that those financial operations are exposed by IPC. Windows release installers are unsigned according to the current README.

No HTTP financial service, OpenAPI contract, API tenancy/authentication layer, PostgreSQL schema, production connector execution runtime, durable outbox or migrations were found in the 146 tracked baseline files. This is a repository-source finding, not a claim about undisclosed private deployments. The baseline has a site and desktop shell; do not replace either unnecessarily.

## Open work and governance

Open PR collection at inspection contained PR #1: `chore(deps): update sha2 requirement from =0.10.9 to =0.11.0`. The open issues endpoint contained that PR and no separate open issue. This package does not restart or modify that dependency work.

Live main's Core CI, Desktop CI, Repository Identity and Desktop Release runs succeeded. Jev/OCR comment-triggered runs on main can be skipped: skipped means no qualification, not passed review. Evidence includes run 37629578695 (core), 37629578692 (desktop), 37629578734 (identity), and 37630861473 (release).

Existing workflows:

- Core: fmt, Clippy, workspace tests and 1.85 MSRV check.
- Desktop: frontend/IPC/native qualification and build checks; release workflow creates packages/checksums.
- Identity: `.github/scripts/check-repository-identity.py` validates CoFi package/product/repository identity and external GitHub links against a narrow allowlist.
- Alibaba OCR delegation: `/ocr-delegate` generates preview/rules and exact base/head scope. This workflow is delegation evidence, not a completed semantic review by itself.
- Genuine TypeSafe Jev: `/jev-review` runs a pinned review runtime with the Actions TypeSafe credential and base-controlled adapter; requires complete exact-diff coverage and no blocking findings.

The rulesets endpoint returned `[]`. Branch-protection read returned 403 `Resource not accessible by integration`; protection status is unknown. CONTRIBUTING.md and PROJECT_STATUS.md require normal merges and exact-head qualification regardless of API visibility.

Research citations use explicit source coordinates and pinned raw/API URLs; they are evidence references, not alternative CoFi product destinations. The current identity checker is preserved. Future imports need a narrowly governed provenance-reference policy, not a broad donor allowlist in user-facing surfaces.

## What this inspection did not prove

No load test, crash test, database recovery drill, provider certification, live payment or exhaustive security audit was performed. Existing CI results qualify their recorded revisions and job scopes, not proposed persistence or a future platform. Main and all upstream pins must be revalidated at implementation start. [Evidence register](SOURCE_EVIDENCE.md) records reproducible source coordinates and coverage limits.
