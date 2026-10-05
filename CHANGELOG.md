
# Changelog

All notable changes to CoFi are documented here.

## [Unreleased]

No unreleased changes.

## [1.0.2] - 2026-10-06

Standalone CoFi repository baseline and full release requalification.

### Changed

- published a clean CoFi-native repository baseline with a single canonical root;
- refreshed repository links and release metadata for the standalone public project;
- requalified the full 21-crate workspace on Rust stable and Rust 1.85.

## [1.0.1] - 2026-10-06

Repository presentation and distribution cleanup.

### Changed

- streamlined the public repository around CoFi runtime and documentation;
- added professional open-source project documentation, security guidance, contribution workflows, and repository metadata;
- reduced the current source tree to the maintained CoFi workspace and project-owned support files;
- preserved runtime behavior while requalifying the exact release head.

## [1.0.0] - 2026-10-06

First stable release of Community Finance / CoFi.

### Added

- native double-entry ledger with typed accounts and idempotent financial commits;
- Community, organization, party, membership, and shared-fund domain;
- billing, receivable recognition, payment capture, and processor payout accounting;
- fund allocation, transfers, deterministic revenue distribution, governance, and spending;
- disbursement lifecycle, provider contracts, reconciliation, and tamper-evident audit;
- usage metering, rating, invoicing, subscriptions, and commercial authorization lineage;
- authorization boundaries for rating, invoicing, finalization, payment, payout, allocation, and transfer;
- Rust stable and Rust 1.85 qualification gates;
- exact-head semantic review and guarded merge workflow.

### Guarantees

- exact replay is idempotent;
- conflicting identity reuse fails closed;
- financial mutations use checked integer arithmetic;
- organization scope and currency boundaries are validated before mutation;
- unsafe Rust is forbidden by workspace policy.
