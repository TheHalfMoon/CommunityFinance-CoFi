# Hyperswitch import plan

Pinned source: `juspay/hyperswitch` at `c64c2365f6f931e5f02ca3d6b98576a3e3c7b9e3`, inspected 2026-10-07. Root LICENSE is Apache-2.0; NOTICE is present. Founder permission reference: `FOUNDER-HYPERSWITCH-REUSE-2026-10-07`, based on the founder's statement in this task. Keep private documents private. No source is imported by this documentation change. Modes below are proposed bounded work, not completed qualification.

## Decision: reuse execution knowledge, preserve economic ownership

The connector library is substantial and valuable, but not a standalone CoFi domain dependency. `hyperswitch_connectors/Cargo.toml` depends on workspace models, cards, common types/enums/utils, domain models, interfaces and router environment; transport/runtime dependencies reach Actix, OpenSSL and HTTP clients. Directly copying its Stripe transformer would bring a large type and macro closure. Preserve CoFi's current pure Rust domains. Qualify one execution adapter behind the [connector contract](../architecture/CONNECTOR_PLATFORM.md), not a second ledger or autonomous router.

First experiment compares two bounded choices: an isolated pinned Hyperswitch execution sidecar with fixed connector/account and orchestration/retries disabled, versus a selective port of the required Stripe authorize/capture/readback/refund/webhook subset. Include Prism as a third candidate only after its integration closure is inspected and build/conformance tested. Measure dependencies, reproducibility, secret access, ambiguity behavior, operational services and patch burden. Sidecar is preferred when the closure is large and orchestration can demonstrably be constrained; port is preferred for a small independently testable flow. Reject a sidecar that can silently retry/fallback or whose idempotency cannot bind to CoFi's attempt. Do not run both backends for the same attempt.

## File/module decisions

All paths in this table are relative to the pinned main Hyperswitch source. `PORT` preserves a useful behavior through CoFi's types; `ADAPT` changes interfaces and ownership; `COPY` is limited to licensed sanitized fixture content after qualification; `SIDECAR` is a separately deployed executor; `REFERENCE` is design evidence; `REJECT` excludes the source from the proposed CoFi implementation. The reuse matrix adds pins/licenses for ecosystem repositories.

| Source unit | Mode | CoFi target and precise boundary | Qualification and exclusions |
|---|---|---|---|
| `crates/hyperswitch_interfaces/src/api.rs` | EXTRACT CONTRACT / ADAPT | Flow-specific execute/readback capabilities under provider-contract | Do not import all generics/models; explicit unsupported flows and amount units |
| `crates/hyperswitch_interfaces/src/webhooks.rs` | EXTRACT CONTRACT | Verified receipt -> typed observation boundary | Bind exact raw bytes, provider account, operation, secret version; verification is not accounting |
| `crates/hyperswitch_connectors/src/connectors/stripe.rs` | PORT or SIDECAR | First card authorize/capture/void/sync/refund adapter | Pin Stripe version; amount conversion, partial capture and timeout/readback fixtures; no runtime fallback |
| `crates/hyperswitch_connectors/src/connectors/stripe/transformers.rs` | PORT selected mappings | Pure request/response translation | Inspect complete dependency closure, preserve raw source evidence and unknown statuses; no blanket 5,843-line transplant |
| `crates/hyperswitch_connectors/src/connectors/adyen.rs` | PORT or same qualified SIDECAR | Second connector after first is operational | Async webhook verification and capture/refund differences; webhook acceptance not settlement proof |
| `crates/hyperswitch_connectors/src/connectors/paypal.rs` | PORT or same qualified SIDECAR | Later wallet/redirect flow | Provider-authenticated webhook verification and order/capture identities; no assumption identical to cards |
| `crates/connector_configs/src/connector.rs` | ADAPT | Versioned capability manifest | Split supported code from qualified country/account/method capability; redact credentials |
| `crates/api_models/src/payments.rs` | REFERENCE / EXTRACT CONTRACT | Public intent/attempt DTO concepts | Do not expose donor model union as stable CoFi API or map provider status directly to a journal entry |
| `crates/api_models/src/customers.rs` | REFERENCE | Party/customer and provider mappings | One CoFi party identity with separately scoped provider aliases; privacy/deletion retention rules |
| `crates/router/src/core/payments.rs` | REFERENCE; bounded SIDECAR execution only | Identify flow sequencing and provider request behavior | Do not transplant router ownership of payment lifecycle; CoFi owns commands and authority |
| `crates/router/src/core/refunds.rs` | PORT selected mapping/test behavior | Refund attempts linked to captured amounts | Canonical refund ceiling/reservation remains CoFi; concurrent partial refund tests |
| `crates/router/src/core/disputes.rs` | ADAPT provider evidence only | Dispute observations/evidence submission | Preserve liability and accounting rules independently; no assumption dispute event means cash debit |
| `crates/router/src/core/mandate.rs` | EXTRACT CONTRACT / PORT selected connector mapping | Provider-bound consent/mandate reference | Consent, off-session authority and recurring execution are distinct; no automatic migration of mandates |
| `crates/router/src/core/payment_methods.rs` | REFERENCE / provider-backed adapter | Token/method capability and provider binding | Default no raw PAN/CVC in CoFi; method portability only with provider permission/process |
| `crates/router/src/core/payouts.rs` | PORT selected flows later | Vendor/connected-party payout adapter | Recipient eligibility, payout sync, fees and return handling; license/custody qualification precedes activation |
| `crates/router/src/core/webhooks/incoming.rs` | ADAPT bounded receipt behavior | Durable inbox, verification and observation normalization | No database mutation before signature/account/scope checks; duplicates/order/races adversarially tested |
| `crates/router/src/core/webhooks/outgoing.rs` | REFERENCE / ADAPT transport only | Committed CoFi domain event delivery | Independent signing/event versions; no donor event as authoritative CoFi event |
| `crates/router/src/core/payments/retry.rs` | REFERENCE; REJECT direct retry policy | CoFi evidence-bound attempt resolution | UNKNOWN never triggers a fresh charge; characterize issuer declines/soft decline/authentication separately |
| `crates/router/src/core/routing.rs` | REFERENCE / PORT pure predicates | Explainable eligible-provider selection | No routing after ambiguous acceptance, no hidden fallback; fixed-point scores and evidence freshness |
| `crates/router/src/core/routing/helpers.rs` | PORT eligible pure helpers after closure check | Eligibility evaluation | Strip runtime/store coupling; scope and capability tests |
| `crates/router/src/core/routing/transformers.rs` | REFERENCE / PORT bounded DTO maps | Route explanation snapshot | Cost estimate is not booked fee; no floating-point canonical money |
| `crates/euclid/src/lib.rs` | REFERENCE; possible DEPENDENCY later | Restricted declarative routing rules | Prove determinism/limits and independent dependency/license closure before use; no financial authorization bypass |
| `crates/router/src/core/revenue_recovery.rs` | REFERENCE | Billing dunning scheduling | CoFi subscription/invoice/UNKNOWN state decides when a new collection is lawful |
| `crates/router/src/workflows/payment_sync.rs` | ADAPT contract | Provider readback work item | Existing attempt ID/key/account preserved; query absence is not always proof of rejection |
| `crates/router/src/workflows/refund_router.rs` | ADAPT contract | Refund readback scheduling | Retain disputed/unknown exposure until resolved |
| `crates/router/src/workflows/payout_sync.rs` | ADAPT contract | Payout evidence resolution | Returned/reversed payouts append effects |
| `crates/router/src/workflows/outgoing_webhook_retry.rs` | REFERENCE | At-least-once notification worker | Safe event replay differs from unsafe external payment replay |
| `crates/router/src/workflows/invoice_sync.rs` | REFERENCE | External invoice observation import | No imported invoice scheduler replacing CoFi billing truth |
| `crates/router/src/core/payment_link.rs` | ADAPT contract/UI flow later | Revocable scoped checkout link | Bind amount/version/environment, expiry and allowed redirect origins; no donor branding |
| `crates/router/src/core/authentication.rs` | REFERENCE; REJECT canonical authority transplant | Separate authn and CoFi economic authorization | Merchant auth is not community quorum or agent budget authority |
| `crates/router/src/core/fraud_check.rs` | EXTRACT CONTRACT / provider adapter | Risk observations/requirements | Risk unavailable behavior explicit; score cannot grant spend authority |
| `crates/router/src/services/encryption.rs` | REFERENCE | KMS-backed secret handle boundary | Do not port cryptographic primitives; separate keys, audit and tenant scope |
| `crates/diesel_models/src/payment_intent.rs` | REFERENCE; REJECT direct schema | CoFi intent/attempt storage | Domain state/replay schema controls PostgreSQL design, not donor row status |
| `crates/diesel_models/src/merchant_account.rs` | REFERENCE | Legal party/provider account mapping | No duplicate authoritative tenant model |
| `crates/diesel_models/src/merchant_connector_account.rs` | REFERENCE | Connector binding and opaque secret handle | Credentials separated from canonical money/audit exports |
| `crates/storage_impl/src/lib.rs` | REJECT canonical reuse | CoFi PostgreSQL unit of work | Do not add Redis/KV truth or donor storage beside CoFi journal |
| `crates/drainer/src/{handler.rs,stream.rs,query.rs}` | REFERENCE; REJECT as ledger outbox | CoFi transactional outbox/inbox | Redis-stream drainer is not proof of atomic PostgreSQL journal+outbox acceptance |
| `crates/scheduler/src/{lib.rs,scheduler.rs}` | REFERENCE / ADAPT scheduling concepts | PostgreSQL jobs/leases module | No lease expiry interpreted as payment cancellation; no mandatory Redis/Kafka for minimal deployment |
| `crates/events/src/lib.rs` | REFERENCE | Telemetry/event transport concepts | Instrumentation emission is not a durable economic event commit |
| `crates/analytics/src/payments.rs` | REFERENCE | Projection metrics/reporting | Explicit lag/watermark and no alternate balance truth |
| `crates/router/src/core/reconciliation*` | REJECT assumed import | Extend existing CoFi reconciliation | No dedicated implementation at this path was found; scattered references do not prove settlement matching |
| `crates/router/tests/connectors/{stripe.rs,adyen.rs,paypal.rs}` | COPY sanitized licensed fixture units / PORT assertions | Offline connector conformance + separately credentialed sandbox tests | These tests may require real credentials/provider state; do not label copied test files deterministic offline fixtures |
| `postman/`, `postman/portman-config.json` | ADAPT test scenarios | API/connector flow fixtures | Remove secrets, donor base URLs/identity and unsupported flows; qualify exact subset |
| `config/development.toml` | REFERENCE; REJECT wholesale copy | Minimal CoFi environment manifest | No donor branding, service explosion, production secrets or unqualified configuration defaults |

## Ecosystem boundaries

`juspay/decision-engine` is **AGPL-3.0**, not part of the main-repo permission. Its cost/success routing and trace concepts are references only; independently implement bounded deterministic rules. Do not import it into Apache CoFi or assume an RPC boundary automatically resolves obligations.

`juspay/hyperswitch-prism` at `c9c0727d99ada0f0eda057f3d8a6f16719a43bbc` has an Apache-2.0 root LICENSE retrieved directly. Windows checkout failed on an upstream trailing-space filename, so this study used the pinned remote tree and selected raw source. `crates/types-traits/interfaces/src/connector_integration_v2.rs` separates generic connector integration; `crates/types-traits/domain_types/src/router_request_types.rs` and `crates/integrations/connector-integration/src/connectors/stripe.rs` expose flow/request mapping; `crates/grpc-server/grpc-server/Cargo.toml` still pulls a substantial common/type/service closure and a Hyperswitch `injector` Git tag. It is a promising SIDECAR candidate, not an already proven drop-in library. Pin the transitive Git dependency by resolved commit and qualify gRPC authorization, logging, idempotency and retries before choosing it.

`hyperswitch-web` is an optional provider-bound checkout DEPENDENCY/ADAPT candidate. `hyperswitch-control-center` is a UI/workflow REFERENCE; root LICENSE and package metadata disagree (Apache versus MIT), so file rights must be reconciled before copying. Preserve CoFi Desktop identity and localization. Card vault and encryption service are optional isolated SIDECAR candidates only after PCI/key-management qualification, not default deployment requirements. Their own pins and permission bases are in the reuse matrix.

## Import record and maintenance

Each future import unit records repository/revision, upstream path/blob hash, local path/hash, original license/notices, permission reference, modifications, owner, dependency closure, tests, security/PCI scope, divergence and patch tracking. Planned target is `crates/cofi-service/src/connectors/<provider>/` for boundary orchestration, with pure mappings initially under provider-contract; the physical split is decided by measured dependencies. Provenance/legal notices live in dedicated records/NOTICE entries. Keep required attribution intact; remove product branding from product surfaces without falsifying authorship.

Every upgrade starts pending qualification. Compare mapped status changes, amount conversions, webhook verification, authentication/mandate behavior, readback and provider key retention. Apply security patches promptly with reviewed impact; retain the old connector for readback of historical attempts if schema/behavior requires it. A connector maintainer must own upstream monitoring and failures; a copied file without an owner is operational debt.

Release qualification: reproducible pinned build; license/dependency closure; CoFi identity; offline fixtures and qualified provider sandbox; scope/credential isolation; UNKNOWN/crash/readback tests; partial capture/refund ceilings; signed raw-webhook verification; duplicate/order handling; audit redaction; deployment/upgrade/rollback drill. Source maturity reduces mapping work, not CoFi's obligation to prove these properties.
