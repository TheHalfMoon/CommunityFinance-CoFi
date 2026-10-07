# Security, operations and regulated boundaries

Status: proposed threat/control/qualification plan, not a completed security scan or compliance certification. Preserve SECURITY.md and existing disclosure process. Financial correctness, authorization, data privacy and recoverability are joint release properties; no single dependency or signature proves all four.

## Threat/control matrix

| Threat | Required control | Qualification / operational response |
|---|---|---|
| Stolen API key | Hashed scoped keys, rotation/revocation, live role/grant check, per-principal limits; keys never imply arbitrary ledger access | Cross-environment/resource and revoked-key tests; revoke, freeze affected queued commands and inspect accepted attempts |
| Leaked connector credential | Opaque secret broker handles, tenant/provider/account/egress scope, least-privileged provider keys, redacted logs | Broker/egress tests, secret-scanning and rotation drill; reconcile before reenabling credential |
| Replay | Distinct durable API/domain/provider/observation/event identities; economic tombstones retained | Changed duplicate conflicts, response-loss/process-restart tests; never clear keys to repair an UNKNOWN payment |
| Webhook forgery | Verify exact raw bytes, secret/certificate version and provider account; bounded body/timestamp handling | Forged/altered/rotated-signature fixtures; failed verification creates no financial transition |
| Webhook reordering | Evidence-bound transition rules, cumulative amounts/qualified readback, no universal last-timestamp-wins | Permuted partial/capture/refund/dispute events; quarantine contradictions |
| Duplicate delivery | Durable inbox event/semantic dedup, linked raw evidence | Duplicate IDs and distinct IDs for same economic fact produce one effect |
| SSRF | Fixed qualified connector endpoints, validated redirect/DNS/IP/egress controls, block private/metadata ranges and rebinding | IPv4/IPv6, redirect chain and DNS-rebinding tests; no caller URL used as credential destination |
| Connector compromise | Isolated executor with scoped secrets/network, authenticated bounded RPC, no canonical DB write credentials | Malicious/out-of-scope result tests, worker egress isolation; kill executor, freeze affected attempts, readback independently |
| Tenant breakout | Credential-derived context, composite FKs, scoped queries/queues/secrets/exports plus RLS defense | Multi-tenant negative tests and pool context reset; runtime uses non-owner/non-superuser role |
| Malicious plugin | No in-process arbitrary plugins by default; signed/pinned packages, capability manifests and resource limits | Connector conformance/provenance/SBOM; sandboxed pure extension/WASM only if justified and qualified |
| Insider access | Separate runtime/admin/approver/auditor roles, explicit break-glass, immutable access/economic evidence and witness | Privileged access/export drill; emergency authority freezes rather than fabricates quorum |
| Ledger tampering | Append-only history permissions, hashes/checksums, recomputed balances and external witness/backup comparison | Tamper/missing-row tests; quarantine and append approved correction, no destructive edit |
| DB corruption | Consistency verification, checksummed backup/WAL/PITR, primary-authoritative acceptance, integrity monitoring | Restore/replay drill for all registries; disable dispatch until state and external exposure verified |
| Supply-chain attack | Exact source/dependency pins, licenses/SBOM, reproducible artifacts and narrow CI permissions | Import/upgrade diff review, signed build evidence, dependency/risk checks; qualification never inherited from upstream label |
| Dependency takeover | Ownership/publish/script change screening, lockfile review, network-isolated builds where possible | Quarantine changed closure and upgrade on feature branch; no unattended finance dependency promotion |
| Poisoned migration | Checksums, reviewed expand/backfill/switch, separate migration role and no runtime DDL privilege | Interrupted/backfill/rollback-as-new-migration tests, restored snapshot validation |
| Prompt injection causing spend | Model can propose only; deterministic live grants/approval/reservation/dispatch checks | Injected invoices/messages/tool results cannot change beneficiary/budget/approval; no model-produced authority token |
| Malicious merchant metadata | Treat as data, bounded schema, escaped UI, separate from policy; verified beneficiary IDs | XSS/log injection and agent data-isolation tests; metadata does not define account/currency/status |
| Fraud-provider failure | Versioned risk requirement/TTL; fail closed when policy requires a result, explicit low-risk bypass only in preapproved policy | Timeout/stale/disagreement tests; risk service cannot authorize money by itself |
| Partial external success | Separate intent/attempt/capture/refund/settlement amounts and held exposure | Partial execution/crash fixtures, matched observations and accounting effects; unresolved exposure has an owner |
| Network partition | Uncertain COMMIT resolution on primary, UNKNOWN after ambiguous external dispatch, no fresh fallback key | Cut network at every acceptance boundary; no assumption that timeout equals failure |
| Clock skew | Trusted service/database time, fixed accepted logical time, bounded token/event skew and expiry policy | Forward/backward clock tests; timestamps are not sole provider ordering authority |

## Data and cryptography

Use reviewed TLS, standard signing/verification and KMS-backed envelope encryption; no new cryptographic primitives. Separate data, secret and signing keys by purpose/environment; rotate with versioned records and recovery procedures. Audit hashes detect alteration only against a trusted witness; a database administrator who rewrites both data and an unwitnessed chain can evade detection. Store signed/witnessed chain heads externally with retention and compare after restore. Witnessing does not prove the original transaction was authorized or true.

Keep raw webhook/provider evidence encrypted with bounded access/retention, storing hashes and parsed lineage in canonical records. Never log PAN/CVC, secret keys, bearer payment tokens or full sensitive payloads. Retention partitions personal data from durable financial facts where possible; legal deletion/anonymization obligations do not justify silent history rewriting. Exports require explicit role/scope and leave evidence. Test data is synthetic and cannot route to live endpoints.

## Minimal operations and failure modes

Support one PostgreSQL primary and one application binary in API/worker roles; optional qualified executor only. No distributed active-active financial writers initially. Primary HA/failover is allowed only after fencing and uncertain-commit resolution tests. A read replica/projection can answer dashboards but cannot authorize money using stale balance/grant state. On unsafe primary state, fail closed for mutation while keeping permitted diagnostics available.

Health endpoints distinguish process liveness, database/migration readiness, worker lease health and financial safety readiness. Alert on UNKNOWN age/count, pending settlement/reconciliation discrepancy, scope/invariant violations, audit witness mismatch, outbox lag/dead letters, usage aggregation cutoff lag, migration failures, backup/WAL age, grant/reservation anomalies and provider error/verification rates. Trace request/operation/attempt/observation/journal IDs with scope-safe redaction. Logs/metrics are not financial truth. An operator must be able to find the owning case from an alert.

Provisional operational targets: alerts for safety invariant failures immediately, unresolved dispatch ambiguity investigated within 15 minutes during supported operation, daily backup verification plus periodic fresh-instance/PITR drills, RPO <=5 minutes/RTO <=60 minutes for supported deployment. These are proposed targets to validate under G007/G008 and pilot support, not delivered SLAs. Define actual support hours and escalation owner before live launch. For longer settlement rails use rail-specific unresolved-case windows, not a universal timeout-to-failure rule.

Incident sequence: freeze new affected dispatch, preserve evidence and current attempt identities, revoke compromised access, determine committed canonical state, independently reconcile external exposure, append approved corrections, verify restored projections/authority and only then resume qualified flows. Break-glass operations are audited and narrow. Do not bulk resend queues after a restore, failover or provider outage. A recovered old grant cannot silently regain revoked authority.

Analytics starts as rebuildable SQL projections/reports with watermark. Warehouse/ClickHouse adapters are optional later. Report invoice/revenue/fee/cash, provider attempts, approval budgets and discrepancy counts separately. A provider approval-rate or estimated routing cost chart is not canonical profit or settled cash. Performance claims require workload, hardware, data volume, failure profile and tested configuration.

## Regulated responsibilities and activation checklist

Acquire/process/hold funds only through qualified licensed providers and actual eligible customer/provider accounts. CoFi's role as software does not guarantee exemption if product/control/custody changes. Before each live region/flow, an accountable legal/operator review records legal entities, contract/liability allocation, custody/beneficiary ownership, onboarding/KYC/sanctions responsibility, consumer/privacy/retention rules, tax/e-invoicing rules and chargeback/refund obligations. Product engineering cannot infer these from an API field.

For Saudi use cases, SAMA's [Payments and Payment Services Law implementing regulations](https://rulebook.sama.gov.sa/en/implementing-regulations-payments-and-payment-services-law) and [licensing chapter](https://rulebook.sama.gov.sa/en/chapter-3-licensing-requirements-payment-service-providers) are primary scoping references. This plan does not classify CoFi or a customer as licensed/exempt. Define the concrete service and obtain appropriate qualified review before activation. A pending/in-principle provider approval is not an operational authorization.

PCI SSC currently lists [PCI DSS v4.0.1](https://www.pcisecuritystandards.org/document_library/). Tokenized/hosted checkout can reduce exposure, but source code/vault reuse is not certification and does not by itself eliminate PCI scope. Document complete browser/server/provider/vault dataflow, administrative access and scripts; obtain the appropriate scope/assessment for the deployed product. Default CoFi does not receive/store raw card data; an optional vault is a separately qualified operational product.

Fraud decisions, identity/KYC, tax filing, bank data consent, treasury accounts, payouts, issuing and capital are partner capability contracts with evidence/eligibility limits. Absence/stale evidence fails the policy that requires it. Merchant-of-record coverage is a contractual service, not another local Rust crate. Do not launch global custody, issuing, lending, public investment pools or hardware acceptance merely because a reusable API exists.
