# Security Policy

CoFi treats financial integrity issues as security issues.

## Supported versions

| Version | Security support |
| --- | --- |
| 1.x | Supported |
| < 1.0 | Not supported |

## Reporting a vulnerability

Do **not** open a public issue for a suspected vulnerability.

Use GitHub's private vulnerability reporting flow when available. If private reporting is unavailable, contact the repository maintainer privately through GitHub.

Include:

- the affected component and version or commit;
- a minimal reproduction;
- expected and observed behavior;
- potential security or financial impact;
- any suggested mitigation.

Do not include real credentials, customer data, payment data, or other sensitive information.

## Security model

Reports are evaluated against CoFi's core invariants:

- deterministic state transitions;
- exact replay and idempotency;
- fail-closed validation;
- authorization lineage;
- organization scope and currency isolation;
- integer money and checked arithmetic;
- double-entry ledger integrity;
- tamper-evident audit semantics.

A security fix is not complete until it has regression coverage and passes the repository's normal review and CI gates.
