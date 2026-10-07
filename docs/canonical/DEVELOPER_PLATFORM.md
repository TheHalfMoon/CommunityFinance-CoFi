# Developer platform

Status: proposed contracts, not shipped endpoints or commands. Depends on durable state and authenticated tenant/environment context. The first developer journey is a deterministic sandbox plus read-only shadow import; live money requires connector qualification.

## One coherent surface

Use REST/JSON with an OpenAPI contract generated from reviewed wire DTOs, never direct serialization of internal Rust state. Stable resource families are parties, funds, proposals, grants, usage events, prices, subscriptions, invoices, payment intents, attempts, refunds, disputes, settlements, payouts, reconciliation cases and events. Separate external observations from canonical resources. Require server-derived organization/project/environment for every request; clients cannot select ledger postings, authorization outcomes or arbitrary provider status.

Every mutation takes an idempotency key. Store the accepted normalized request, authenticated scope, API version and exact result in the same transaction as the economic effect. Reusing a key with a different normalized request returns a typed conflict. Pure validation failures may be retried after correction; accepted/ambiguous operations keep their identity. Document the distinction. An HTTP timeout never instructs the SDK to create a new economic operation.

Amounts and quantities travel as decimal strings, including values above JavaScript's safe integer range. Currency is a validated registry identifier with an explicit exponent/version; an existing three-letter constructor alone is insufficient. SDK ergonomic helpers may accept checked safe integers but serialize strings and reject lossy numbers. Timestamps use UTC instants; billing schedules additionally retain named time zone and calendar policy. IDs are opaque and type-distinct. Metadata is bounded, untrusted data, excluded from authorization and sanitized in UI/logs. Preserve selected metadata in exact domain replay where existing semantics require it.

Errors contain stable code, request ID, resource/attempt ID when accepted, retry class, safe field details and reconciliation link. Classes include invalid_request, scope_denied, idempotency_conflict, insufficient_budget, unsupported_capability, authorization_expired, provider_action_required, outcome_unknown and temporarily_unavailable. Do not reveal other tenants' existence. Rate limits include bounded ingestion, per-principal mutation and observation polling limits; 429 retries apply to the same key only.

Version contracts by explicit API date plus schema revision, not donor API version. Pin event schema version at emission and webhook subscription version at registration. Support old versions for a published interval and provide fixtures/diffs before retirement. Unsupported versions fail explicitly. Compatibility promises include rounding, pagination, error classes and replay behavior, not just field names. Additive unknown enum states must remain safely representable by SDKs.

Pagination uses signed opaque keyset cursors bound to scope, filters and stable ordering; never mutable offset pagination for accounting history. Lists expose a snapshot watermark or explain ongoing updates. Expansions are allowlisted and bounded in depth/cost. No expansion bypasses resource authorization. Search/analytics may lag and display their watermark; authoritative mutation/readback uses the primary database.

## Proposed first API contract and SDK journey

The initial collection surface is `POST /v1/payment_intents`, `GET /v1/payment_intents/{id}`, flow-specific capture/cancel/refund commands, and `GET /v1/attempts/{id}`. `payments.create` is the SDK convenience name for creating that intent, not for directly posting cash. Authentication resolves scope/environment; an API-version header and idempotency key bind the accepted request. Response includes intent ID, exact amount/currency, status, accepted command ID, current attempt ID (if any), required next action and request ID. Only supported fields/flows are accepted.

Illustrative **future sandbox** TypeScript contract:

```typescript
const cofi = new CoFi({ apiKey: process.env.COFI_SANDBOX_KEY });
const payment = await cofi.payments.create({
  amount: "10000",
  currency: "SAR",
  customer: "party_example",
  paymentMethod: "pm_sandbox_example",
}, { idempotencyKey: "example-order-001" });
// Inspect payment.status and nextAction; acceptance is not settlement.
const usage = await cofi.usage.ingest({
  sourceEventId: "usage-example-001", subject: "party_example",
  meter: "input_tokens_v1", quantity: "1200",
  occurredAt: "2026-10-07T00:00:00Z",
}, { idempotencyKey: "usage-example-001" });
```

The example requires seeded sandbox parties/methods/price contracts; it does not imply SAR eligibility for a live processor. Subsequent examples create a subscription from a published price contract, simulate/finalize an invoice with separately authorized authority, and submit `funds.proposeSpend`/`agent.proposeSpend`. An agent receives a proposal/needs_approval result rather than an approval token. If the payment returns outcome_unknown, SDK guidance is to retrieve the same intent/attempt or retry the same logical API key, never create a replacement charge. All these endpoints/methods are proposed and must be generated/tested from one released contract before appearing in runnable docs.

## SDK and CLI priorities

TypeScript and Python SDKs first; Rust next for infrastructure users; Go, Java, .NET, PHP and Ruby based on integration demand. Generate resource DTOs and basic transport, handwrite the small layer for safe retry, typed ambiguous outcomes, pagination, event verification and decimal values. Shared conformance fixtures must produce identical wire bytes and semantic results across languages. Do not ship seven shallow SDKs before two usable ones. Publish signed packages with provenance and dependency policies.

The target quickstart is `cofi init` then `cofi dev`, followed by one sandbox payment and one usage-to-invoice example. This is a five-minute target after prerequisites, not a claim about today's desktop. `cofi dev` starts a pinned local service, PostgreSQL, deterministic test processor and dashboard; packaged/containerized dependencies must be supplied explicitly. No hidden cloud account or live credential is required. A clock-control API exists only in sandbox and cannot be enabled on a production database.

Planned CLI commands: `cofi fixture load`, `cofi events listen`, `cofi doctor`, `cofi import plan`, `cofi import validate`, `cofi import shadow`, `cofi reconcile`, and `cofi export verify`. Sensitive commands show scope/environment and require a preissued principal with the correct role; a confirmation dialog cannot grant missing authority. Doctor defaults to read-only verification. Repair produces an approval-bound plan, never directly rewrites journal history.

Docs must distinguish API request acceptance, provider acceptance, customer completion, settlement and available funds. Every guide contains success, duplicate replay, invalid input, timeout/UNKNOWN and recovery examples. Maintain a capability matrix for each connector, supported country/currency/method/flow, provider account eligibility, sandbox differences and known limitations. Provide a versioned glossary, financial state diagrams, troubleshooting by request ID, operating procedures, migration recipes and RTL/localization guidance.

## Deterministic sandbox and webhook laboratory

The test processor implements the same connector contract as live processors. Scenarios include authorize/capture, delayed settlement, partial capture/refund, expiration, duplicate/out-of-order notifications, signature rotation, invalid signatures, readback disagreement, rate limit, provider decline, network loss before/after acceptance and unknown outcome without readback support. Seed IDs and clocks explicitly. No random sleep controls expected results. Sandbox signing secrets and provider handles are unusable in live mode.

Outbound events come only from the committed durable outbox. Sign timestamp, event ID and exact payload bytes with a scoped rotating secret; consumers check signature, allowed timestamp skew and durable event ID deduplication. Retain retries and dead-letter evidence, expose delivery status and manual replay with the same event identity. A notification success never proves a payment succeeded. Local listener receives sandbox events or explicitly authorized redacted live events through an authenticated tunnel; no arbitrary public forwarding of secrets or personal data.

The dashboard is a trace reader plus controlled operator interface: intent -> authority decision/reservation -> attempt -> verified observation -> journal entry -> settlement -> reconciliation case. Show amount/currency, environment, status uncertainty, source evidence and projection freshness prominently. Never turn an optimistic loading state into a financial success. Existing Tauri Desktop remains a client of the same application contract; preserve seven locales and Arabic RTL. The current two native commands are not replaced by invented finance features.

## Migration and Stripe compatibility

Compatibility is an explicitly limited adapter, initially create/read one-time intent, capture/refund and supported event mappings. Pin the donor API version and publish unsupported fields/flows. A base-URL change alone cannot preserve Stripe Elements, tokens, Connect liability or billing semantics. Unsupported operations fail; never silently fall through to a processor.

Migration sequence: inventory versions/provider accounts, acquire scoped read-only access, stage immutable import records with source IDs/hashes, validate currency/exponents and relationships, construct a reviewed opening-balance plan, compare shadow projections for at least one complete billing/settlement cycle, then switch one new-write cohort. Historical invoices stay evidence-linked; do not rerun charges or recalculate issued invoices. Export canonical state, observations, import mappings, audit chain, grant lineage and versioned policies. Secrets/tokens are excluded or exported via a separate authorized portability process.

Payment methods may be provider-bound and non-exportable; mandates/consent and merchant-of-record obligations are not portable JSON. Existing subscriptions can remain provider-executed until consent and account contracts permit transfer. Rollback freezes new CoFi dispatch, reconciles all outstanding attempts and directs only future unsubmitted intents to the old system. It does not erase already executed money or restart UNKNOWN attempts.

Acceptance: ten external developers reproduce the quickstart with measured median <=5 minutes after prerequisites and >=80% unassisted completion; a fresh environment reproduces fixture output; TS/Python replay safely through a lost response; an export imports into an empty instance with equal verified economic state. These are pilot gates, not measurements obtained in this research.
