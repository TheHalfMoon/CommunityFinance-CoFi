
# Project Status

**Status:** Stable
**Current release:** v1.0.2
**Language:** Rust
**MSRV:** 1.85
**License:** Apache-2.0

CoFi v1.0.2 is the current stable release of the Community Finance core. Desktop v1.1.0 is the current release candidate and is not yet published as the stable release.

## Stable capabilities

- native double-entry ledger and account registry;
- deterministic billing and receivable recognition;
- payment capture and processor payout accounting;
- Community shared funds, allocation, transfers, and revenue distribution;
- policy-based governance and approved spending;
- vendor disbursement lifecycle and provider-neutral contracts;
- reconciliation and tamper-evident audit;
- usage metering, rating, invoicing, and subscriptions;
- authorization lineage through commercial and fund-movement boundaries.

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
