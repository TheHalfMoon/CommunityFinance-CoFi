# Connector platform

Extend `cofi-provider-contract` without breaking its current disbursement lineage. Its existing request/observation contract is not a generic acquiring engine. Introduce payment flow modules and a versioned wire contract around trusted CoFi-derived commands.

## Contract layers

1. Capability descriptor: provider/adapter revision, supported payment methods/countries/currencies/exponents/flows, capture/refund ceilings, idempotency scope/retention, lookup consistency, webhook verification method, timeouts, cancellation semantics, vault/token portability and operational certification.
2. ExternalCommand: CoFi scope/environment/command/attempt IDs, exact flow and immutable money/counterparty, provider account mapping, opaque token reference, provider idempotency reference, authorization receipt digest, route version and bounded non-authoritative metadata.
3. ExecutionResult: accepted/pending/requires-action/definitively-rejected/unknown, provider IDs, safe error class, evidence digest, observed amounts/currency/account and explicit verification/readback facts. No "retry=true" boolean is sufficient financial authority.
4. ProviderObservation: raw receipt digest and provenance, qualified signature/readback verifier revision, normalized event type/effect ID, original provider timestamps/sequence and correlation. The service admits it to canonical state.

Separate flow-specific traits/functions for authorize, capture, void, refund, sync, dispute evidence, mandate setup/revoke, payout create/sync/cancel and settlement import. Optional capabilities return `unsupported_capability`; they never degrade to a different flow. Risk, KYC/KYB, tax, vault, issuing and bank providers use related evidence/request contracts, not an overgrown payment trait.

| Operation | Preconditions | Output authority |
| --- | --- | --- |
| execute capture | Canonical eligible obligation, exact bound authorization, provider capability, amount reservation | Provider evidence only |
| readback | Same provider account/command identity and qualified lookup | Resolution evidence; not automatic approval |
| verify webhook | Raw bytes, configured verifier/secret, provider timestamp/replay rules | Verified envelope; semantic validation still required |
| refund | Canonical capture and refund ceiling including pending attempts | New refund observation |
| import settlement | Source authenticity, cutoff, schema version, account match | Batch evidence, not caller-designed postings |
| tax/risk/KYC | Minimum consented data and declared provider policy | Decision/calculation evidence; CoFi policy determines allowed action |
| vault resolve | Bound token use and exact provider/destination | Opaque reference or isolated sensitive execution, never canonical card data |

## Extension execution choices

| Form | Use | Decision |
| --- | --- | --- |
| Pure Rust crate | First-party deterministic mappings and safe parsers | Preferred after provenance/tests; no credentials in domain |
| Isolated HTTP/gRPC adapter or sidecar | Mature provider engines and third-party extensions | Preferred trust/secret boundary; versioned contract, mTLS and restricted egress |
| In-process third-party plugin | Arbitrary native runtime | Reject for initial platform: shares memory, credentials and failure domain |
| WASM | Later deterministic bounded transforms/rules | Sandbox resource limits and host calls; no direct secret/network/ledger write |
| Generated adapters | Boilerplate from provider schemas | Supplement reviewed mappings; schemas cannot infer payment semantics |

For earliest reuse, qualify a pinned Hyperswitch adapter boundary as an execution prototype and selectively port Stripe mapping/verification/fixtures into CoFi-native adapters when the dependency closure is manageable. These are alternative ways to implement the same adapter, not two executors for one attempt. A selected connector/backend is immutable for the accepted attempt. No engine may autonomously route/retry behind CoFi's recorded command; configuration and contract must disable or expose such behavior. If the engine cannot provide a trustworthy lookup/idempotency boundary, restrict it to sandbox/reference.

Prism is a separately licensed Apache-2.0 ecosystem candidate with gRPC/SDK connector boundaries. Its inspected workspace suggests a potentially cleaner execution sidecar than the full Hyperswitch router. That is a candidate for a bounded conformance spike, not a qualified recommendation to deploy. The separate Decision Engine is AGPL and is reference-only for the default Apache CoFi distribution.

## Secret and PCI boundaries

API/controller/domain/outbox objects carry credential handles, never plaintext secrets or PAN/CVC. A credential broker resolves only the approved provider account, flow and destination in the isolated runtime; process-wide connector credentials for every tenant are prohibited. Rotate, revoke and audit handles. Read-only settlement credentials cannot execute payouts.

Use provider-hosted fields/redirects or a separately operated vault. Tokenization and use of a vault do not automatically remove all PCI obligations. If a connector requires PAN, its execution belongs in a qualified sensitive boundary and is excluded from the default general-purpose money service. Never store CVC. Redact logs, traces, error samples, snapshots and test fixtures; metadata cannot alter a host or credential destination.

## Routing and safe recovery

Eligibility removes unsupported method/currency/region, unavailable provider, prohibited risk/compliance result, merchant restrictions and token/mandate incompatibility. Then deterministic rules can rank success, cost, latency, contractual volume, issuer/BIN and customer history. Use versioned bounded integer/fixed-point scores; statistical estimates are inputs with freshness and confidence, not accounting money.

Persist candidates, rejected constraints, feature provenance/as-of times, rule/version, scores, selected processor, allowed fallback order and reason. A stale/unavailable optimization service falls back to a preapproved deterministic route before send or refuses; it cannot erase an in-flight attempt. A new route after definitive rejection still requires eligible credentials/token, policy, limits and explicit attempt linkage.

No fallback after uncertain acceptance. Retrying a known decline and resolving an unknown payment are separate workflows. Off-session retry requires valid consent/mandate and dunning policy; device/customer action may be required. Model suggestions may inform reviewed policy but never become sole routing or spend authority.

## Qualification and lifecycle

Every connector/flow has a maintainer and status: experimental, sandbox-qualified, live-qualified, deprecated, quarantined. "200 connectors" is not a CoFi supported-flow count. Publish capability matrices by version, provider account geography and payment method.

Tests cover signed webhook verification (including raw-body changes, timestamp/secret rotation, duplicate and reordered delivery), response mapping, amount exponent/overflow, provider error classification, definitive rejection versus ambiguity, accepted response lost, readback eventual consistency, capture/refund ceilings, mandate consent, payout settlement, wrong account/tenant, SSRF/redirects and redaction. Fixtures are synthetic/redacted with provenance; an upstream integration test needing real credentials is not run as a local fixture.

Live qualification also requires provider sandbox certification, permitted contracts/countries, operational ownership, API/version monitoring and documented emergency disable/readback behavior. Changing an adapter is a new qualified revision; pending attempts retain their original interpreter or an explicitly tested compatible migration. Keep raw evidence so improved parsers can explain old results without silently reposting them.

Provider replacement includes customer/external-ID mappings, token portability limits, historical attempt interpreter, outstanding disputes/payouts and rollback constraints. No provider can register a plugin that writes journals directly. See [source import plan](../research/HYPERSWITCH_IMPORT_PLAN.md).
