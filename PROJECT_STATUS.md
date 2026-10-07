
# Project Status

**Status:** Stable
**Current release:** v1.1.1
**Language:** Rust, TypeScript
**MSRV:** 1.85
**License:** Apache-2.0

CoFi v1.1.1 is the current stable repository release. It is a desktop branding patch over v1.1.0 and preserves the qualified Community Finance core behavior.

## Stable capabilities

- native double-entry ledger and account registry;
- deterministic billing and receivable recognition;
- payment capture and processor payout accounting;
- Community shared funds, allocation, transfers, and revenue distribution;
- policy-based governance and approved spending;
- vendor disbursement lifecycle and provider-neutral contracts;
- reconciliation and tamper-evident audit;
- usage metering, rating, invoicing, and subscriptions;
- authorization lineage through commercial and fund-movement boundaries;
- local-first CoFi Desktop shell with typed IPC to the canonical ledger core and Windows MSI/NSIS distribution;
- bundled desktop localization for English, Arabic (RTL), French, German, Spanish, Italian, and Simplified Chinese.

## Quality gates

The repository requires:

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --all-targets --all-features`
- Rust 1.85 MSRV qualification
- exact-head code review for governed changes
- guarded normal merge commits

## Maintenance

Changes after v1.0.0 are treated as normal product maintenance and feature development. Release notes are maintained in [CHANGELOG.md](CHANGELOG.md).
