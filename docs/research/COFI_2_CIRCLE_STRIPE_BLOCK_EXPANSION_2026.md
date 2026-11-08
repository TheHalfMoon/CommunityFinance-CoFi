# CoFi 2.0 — Circle × Stripe × Block expansion research and execution plan

Research: 2026-11-08. Repository: TheHalfMoon/CommunityFinance-CoFi, reviewed main at 6f8d620d90d6b799110d2987b9a10e4e66133a43.
Status: RESEARCH / PROPOSED / NO CODE IMPORTED. This document extends, rather than replaces, docs/canonical/ and the existing source-evidence register. It does not approve production money movement, publish an API, certify a rail, or claim a source-import right.

## Executive decision

Make CoFi the open, portable financial control plane for businesses and developers: canonical economic history, programmable authority, pricing and obligations, proof of provider execution, settlement, reconciliation and exports. Compose regulated execution from licensed partners. The long-term model is:
- Stripe: developer-first checkout, billing, platform payments, operations, risk and embedded-finance integrations;
- Circle: stablecoin payment orchestration, wallets and multichain settlement via qualified providers, testnets first;
- Block/Square: merchant catalog, orders, POS, inventory, omni-channel payments and Cash App Pay through qualified integrations.

Do not attempt to duplicate Stripe's acquiring network, Circle's issuer privileges/cash reserves, or Block's consumer network, bank contracts, hardware and licenses. Do not rebrand a supplier's entire codebase as CoFi or install an independent second production ledger.

**Initial wedge:** read-only shadow finance beside existing PSP(s): import statements and payment events, reconcile all fees/obligations, explain breaks, export a verified evidence bundle. First execution wedge follows only after durable state and a complete single-processor payment/refund/settlement path.

## Grounded current state versus intended capability

Current declared release: v1.2.0; canonical Rust double-entry accounting, billing/rating/metering/subscription/invoicing primitives, payment/payout accounting, governed shared funds, disbursement/provider observations, reconciliation, audit and seven-language Windows Tauri desktop.

Important update since the older 2026-10-07 baseline: current main has a cofi-storage crate with numerous bounded validated codec / reconstruction slices and docs/implementation/G001_CODEC_COVERAGE.md. That coverage explicitly states **PARTIAL IMPLEMENTATION / NOT G001 QUALIFIED**. It does not establish a complete PostgreSQL transactional production money store. Main still has no qualified public financial HTTP API, licensed live processor, qualified issuer/custodian or production merchant POS from the inspected evidence. Several storage hardening PRs #95–#113 remained open when inspected; recheck heads and merges before grains.

Existing research is substantial: docs/research/SOURCE_EVIDENCE.json lists 31 researched repositories; docs/research/SOURCE_REUSE_MATRIX.md already covers Hyperswitch, Prism, OpenMeter, Kill Bill, Lago, Meteroid, Medusa, Vendure and founder-owned assurance/agent tooling. **Avoid double-counting these as newly discovered sources.** Exact existing source revisions in the old research date back to October 7 and need repinning.

## Competitive capability mapping

| Capability | Stripe reference | Circle reference | Block/Square reference | CoFi decision |
|---|---|---|---|---|
| Merchant payment lifecycle | PaymentIntents, Checkout, refunds, disputes, Connect | Fiat/stablecoin pay-in/out through CPN and providers | Payments and online checkout | OWN canonical intents/attempts/refunds/settlement + qualified adapters; one first |
| Billing/revenue | Billing, Metronome, invoices, dunning, tax, revenue recognition | Programmable micropayments | Square Invoices/subscriptions | OWN pricing and invoice truth; tax calculation through provider |
| Stablecoin | Stablecoin and machine/agent products | Wallets, CCTP V2, Gateway, Paymaster, CPN Managed Payments | Bitcoin-related services/commerce | New OPTIONAL asset/rail adapter, never default fiat storage |
| Commerce/POS | Terminal/Payment Links | Wallet checkout | Catalog, Orders, Inventory, POS, Terminal, Cash App Pay | OPTIONAL separate commerce application domain; provider-operated in-person acceptance |
| Platforms/communities | Connect, split transfers, managed onboarding | Cross-border business pay-outs | Seller/merchant ecosystems | OWN approval, beneficiary and liability lineage; licensed provider handles custody/KYC and transfers |
| Risk/compliance | Radar, Identity, Financial Connections | Compliance-integrated network/partners | Merchant/KYC and dispute programs | Screening and case orchestration only; do not claim network fraud model parity |
| Finance operations | Workbench, reports, data sync, treasury | Transaction/chain/settlement evidence | Merchant dashboards, reports | OWN cross-rail traces, case queues, replay/correction proof and portable exports |
| Developer platform | API, SDKs, webhooks, sandboxes, CLI | Wallet/contract/crosschain APIs | Square/Cash App Pay APIs, SDKs | First-class versioned API, SDKs, playground, fake-rail test clocks, operator console |

Official current references:
- https://stripe.com/products
- https://stripe.com/connect
- https://www.circle.com/developer
- https://www.circle.com/cpn/managed-payments
- https://developer.squareup.com/docs/commerce
- https://developer.squareup.com/reference/square
- https://developers.cash.app/cash-app-pay-partner-api/guides/partnerships/partner-with-cash-app-pay

## Architecture to preserve and extend

1. Canonical financial history stays in CoFi's domain model and atomic durable store. Money uses exact integer amounts, currency-specific exponents, qualified ISO currencies/assets and explicitly priced FX/fees. Never use float or merge distinct currencies into one balanced posting.
2. One serializable acceptance transaction persists the domain result, replay identity, authority reservation, immutable audit, original provider reference and outbox. A provider's external confirmation is an **observation**, not arbitrary caller-selected postings.
3. Distinct business intent, attempted execution, provider outcome, chain transfer, bank settlement and recognition events. End-to-end correlation IDs and immutable facts join them, with reversible projections but not rewritten history.
4. Production network acceptance ambiguity is a reserved, unretried UNKNOWN state; only same-identity qualified readback/resend may resolve. No opportunistic alternative PSP or on-chain transfer after timeout.
5. Tenant/org/project/environment identities are structural across DB keys, imports, queues, signing, secrets, UI and exports. Provider tokens/keys never enter ordinary client-side state.
6. CoFi business domains live in Rust; vendor and Go/JS/Python systems integrate through narrow audited sidecar or protocol boundaries. Do not replace cofi-ledger with TigerBeetle/Blnk/Formance or create dual economic truth without a conformance-backed architecture decision.
7. Provider-specific risk, custody, card tokenization, KYC, merchant underwriting, fiat/stablecoin exchange, tax filing, issuer operations and cash-management activities remain with qualified partners unless separately licensed/assessed.
8. Stablecoin path introduces asset network ID, contract/issuer, asset decimal precision, chain ID, wallet custody model, stable chain transfer ID, confirmations/finality, bridge/cross-chain stages, fee quote, risk screening, reorg/attestation and fiat off-ramp lineage. Don't conflate chain-finalized with bank-settled.
9. POS path introduces merchant/location, catalog variant, order/tax/tip, inventory reservation, terminal attempt and receipt. All merchant sale events reconcile to the same canonical settlement model.
10. Financial agents request or simulate under bounded capabilities; models never approve or spend. Immutable purpose/beneficiary/budget signatures, revocation and ancestor-limit reservations are deterministic service checks.

## Newly discovered donor and reference candidates (not in Oct 7 register)

Each item below is a research lead, **not** a verified compatible copied unit. Mode: STUDY = review/test patterns only; PORT-CANDIDATE = consider selected modules/test fixtures after pin, unit rights, dependency closure and security conformance; PARTNER = use official APIs/SDKs under provider contract. Re-check repo head, actual license at exact path, maintenance, dependency chain, and safety before any import.

| Project | Target capability in CoFi | Preferred mode | Critical boundary |
|---|---|---|---|
| https://github.com/tigerbeetle/tigerbeetle | Concurrency/transfer/idempotency/failure testing for ledger | STUDY | Different ledger storage model; avoid parallel canonical balance engine |
| https://github.com/blnkfinance/blnk | Inflight transfers, historical balances, matching, financial dashboards | STUDY | Already owning internal ledger; borrow test scenarios, not storage truth |
| https://github.com/formancehq/ledger | Programmable ledger DSL, distributed consistency, observability | STUDY | DSL and distributed cluster are not automatic CoFi invariants |
| https://github.com/moov-io/ach | ACH file parser/writer/validation | PORT-CANDIDATE, later | U.S. ACH format does not grant ODFI membership or origination rights |
| https://github.com/moov-io/rtp20022 | RTP/ISO 20022 message shapes and fixtures | PORT-CANDIDATE, later | Scheme participation and version certifications separate |
| https://github.com/moov-io/iso8583 | Card-network message parsing/fixtures | STUDY | Not a replacement for acquirer/network access or PCI boundary |
| https://github.com/moov-io/watchman | Sanctions/watchlist screening and explainable hits | Optional SIDE-CAR, later | A match is not a compliance decision; data freshness, false positives and private-network deployment |
| https://github.com/mojaloop/central-ledger | Clearing/deferred-net-settlement and participant position concepts | STUDY | Scheme-level regulated network; not app-level payment gateway |
| https://github.com/interledger/rafiki | Open Payments, wallet-provider settlement and GNAP grants | STUDY / qualified SIDE-CAR | Authors say production use for regulated FSPs only |
| https://github.com/interledger/open-payments | Payment request, incoming/outgoing grants and protocol semantics | STUDY/ADAPT-contract | Depends on wallet/account servicing institution |
| https://github.com/interledger/open-payments-rust | Rust Open Payments types/contracts | STUDY/ADAPT-contract | Validate current API and consent security |
| https://github.com/x402-foundation/x402 | Agent/API paid requests and microtransactions | PORT-CANDIDATE, sandbox | Payment proofs, replay, merchant consent and fee/chain risks |
| https://github.com/circlefin/evm-cctp-contracts | CCTP V2 ABI/events, attestation and cross-chain tests | STUDY / official-integration | No claim to Circle issuance or independent mint authority; CCTP V1 deprecated |
| https://github.com/circlefin/stablecoin-evm | Stablecoin issuer/token-event formats and test fixtures | STUDY ONLY | Do not deploy Circle's issuer logic as a CoFi stablecoin |
| https://github.com/btcpayserver/btcpayserver | Noncustodial Bitcoin invoices, receipts and merchant checkout | Optional PARTNER/SIDE-CAR | Separate risk, custody, tax, confirmations and retail suitability |
| https://github.com/flexprice/flexprice | Credit packs, hybrid plans, entitlements, proration, dunning UX | STUDY | Open-core/AGPL/enterprise boundaries require per-unit assessment |
| https://github.com/flexprice/flexprice-front | Billing/admin interaction examples | STUDY | Independently assess source and dependency licenses |
| https://github.com/square/connect-api-examples | Square OAuth, Orders, Catalog, Payments and merchant lifecycle | PARTNER/SDK | SDK samples only; merchant/provider account and eligibility required |
| https://github.com/square/web-payments-quickstart | Tokenized browser checkout/Wallet/ACH samples | PARTNER/SDK | No raw card handling, Square credentials and region eligibility |
| https://github.com/apache/fineract | Bank-account and lending product domain references | STUDY, distant | Not suitable for automatic deposit or lending launch |
| https://github.com/apache/fineract-consumer-facing | Segregated authenticated consumer API/BFF design | STUDY | Keep bank credentials out of the client |
| https://github.com/frappe/erpnext | Merchant inventory, POS, order/invoice workflows | STUDY | Large unrelated ERP dependency and copyleft review |
| https://github.com/solidusio/solidus | Order, inventory, refund and seller-service workflows | STUDY | Commerce domain must remain separate from CoFi ledger |

**Existing top donor:** https://github.com/juspay/hyperswitch — qualify bounded adapter / sandbox execution before accepting any external economic effect. Existing alternatives OpenMeter/Kill Bill/Medusa remain from docs/research/SOURCE_REUSE_MATRIX.md. Source count refers to repositories, not imported/validated modules. No live build/benchmark/code copy occurred for newly discovered sources in this task.

## Delivery order — evidence-based gates, no artificial timelines

| Phase | Outcome | Bounded implementation | Exit evidence |
|---|---|---|---|
| R0 / repo refresh | Establish exact current truth | Revalidate PR #95–#113, G001 codec inventory, current cargo checks and baseline, source revisions | No unreviewed concurrent storage PR overwritten; source matrix dated and reproducible |
| P0 / finish durable foundation | Recover every accepted financial and authority fact | Complete G001; then G002–G008 as already specified: Pg schema, full hydration, serializable unit of work, replay after lost COMMIT, inbox/outbox/UNKNOWN, verified restore | No double economic effect under process kill, network ambiguity, concurrent account/grant races; fresh-DB restore digest parity |
| P1 / developer control plane | An honest self-hostable sandbox financial API | Authenticated org/project/environment/roles, versioned HTTP/OpenAPI, TS/Python clients, CLI, deterministic fake PSP, tenant-safe secret broker | Ten outside developer runs; >=80% can complete sandbox first payment in <5 minutes (TARGET, not observed); no unscoped finance routes |
| P2 / shadow reconciliation | Useful without moving any money | Read-only Stripe + Square import of payments, statements, fees, payout files, order mappings; case center, immutable source receipts, CSV/JSON verified export | At least three design partners; one full settlement cycle per pilot with every discrepancy classified and quarantined; no claimed bank confirmation from webhook |
| P3 / CoFi Pay | First truly safe card/hosted payment flow | Hyperswitch selective bounded adapter or sidecar; hosted/tokenized checkout, intent/authorize/capture/refund/void, dispute observation, payout/settlement | One provider, one eligible account/method/currency/region; crash+restore+UNKNOWN matrix; independent security and partner certification |
| P4 / CoFi Revenue | Complete merchant SaaS cash cycle | Existing Rust meter/rating + flat/seat/usage, tiers, prepaid/promotional credits, immutable invoices, proration, entitlements, dunning, customer portal | Every price/usage/credit/tax/payment/fee and settlement maps to canonical evidence; failed collection never double-charges |
| P5 / CoFi Platform | Governance-native marketplaces | Seller subaccounts via qualified PSP, split obligations, reserves/negative-balance liability, budget approvals, vendor payouts, bounded agents | Provider KYC/custody obligations clear; two budget cycles and multi-party failure/refund conformance; no unlicensed pooled investment |
| P6 / CoFi Stable | Circle-inspired stablecoin rail behind optional adapters | CPN / Circle Wallets / CCTP V2 / Gateway partner feasibility, testnet wallets, quote/bridge/chain finality, x402 optional; full on/off-ramp ledger mapping | Testnet-to-fiat mock complete; adversarial chain reorg, fee drift, chain mismatch, delayed attestation, finality and sanctions checks; real movement only after legal/provider gates |
| P7 / CoFi Commerce | Block/Square-inspired merchant workflow | Merchant/location catalog, orders, inventory, omnichannel reconciliation; Square APIs as first provider, optional POS UI and terminal contract | Sale/refund/chargeback/tips/tax/inventory all trace to settlement; terminal/method/device country capability explicit |
| P8 / scale & enterprise | Portability verified in operation | Second PSP, routing with held UNKNOWN, fraud/risk vendor adapters, bank/ACH/ISO formats, SSO, SLA, observability, DR, warehouse exports | Two processors same tests; provable cutover without duplicate execution; HA/DR/load/incident drills; country-specific permissions |

P3 and P4 can have design parallelism after P0/P1 but live collection waits for gates. P6/P7 stay strictly optional until one successful vertical and customers justify them.

## Near-term implementation slices to assign

1. **R0-A:** Refresh current main and open storage PR topology, owner decisions, exact-head CI, G001 missing-registry list. Deliver short signed evidence report. No copying.
2. **P0-A:** Finish omitted bounded durable codec/registry paths with original constructor replay; preserve existing system identity, arithmetic and full authority ancestry.
3. **P0-B:** PostgreSQL schema + integration harness and independent process recovery; exact integer serialization and tenant/environment scoping.
4. **P0-C:** Atomic all-registry unit of work, economic replay IDs, outbox and UNKNOWN fake-processor attempt state machine; include kill-after-commit tests.
5. **P1-A:** Single resource versioning contract, no public API until formal auth/multi-tenant model exists.
6. **P2-A:** Read-only provider transaction mapping with immutable inbound evidence and quarantine, *no automatic balances or refunds*. Start with one partner and signed file fixtures.
7. **P3-A:** Hyperswitch readback and refund conformance experiment; separate internal attempts from provider IDs. Reject an executor that does hidden retry/routing.
8. **P6/P7 research only:** Circle V2 contract/testnet and Square merchant/order payload fixtures; do not place stablecoin or POS logic into the core money kernel.

Each slice has its own file-level import inventory, owner, original source pin, tests, benchmark/provenance, independent review, release status and rollback/freeze operation. If an imported unit is eligible to reuse, retain lawful attribution and a provenance receipt; renaming never changes ownership or supplier claims.

## Non-negotiable safety, legal and operations

- Code availability and permission to study are separate from copy/redistribute rights. The source research may ignore license *prioritization*, but code import/rebrand needs per-file permissions, transitive licenses, notices, and export/redistribution review. Do not assume permission to copy all Circle/Stripe/Block services merely because API examples or contracts are public.
- Circle CPN Managed Payments is a provider-operated service, not a substitute for issuer/custody credentials. CCTP source is not Circle's private platform, issuer relationship or permission to mint redeemable USDC.
- Cash App Pay's PSP Partner API requires approval. Square examples do not provide merchant acquiring.
- Keep card data out of CoFi by default; use hosted/tokenized collection. PCI scope must be formally assessed before changes in payment-data custody.
- Every actual money rail and launch region requires legal operator/PSP agreements, screening/risk roles, consumer protection, incident operations, and accounting treatment. See https://rulebook.sama.gov.sa/en/implementing-regulations-payments-and-payment-services-law and https://rulebook.sama.gov.sa/en/rules-dealing-e-commerce-payment-service-and-support-providers for Saudi-specific scoping; determine actual role with qualified counsel.
- Include freezes, reconciliation cases, privacy minimization, secrets rotation, audit integrity, sandbox/live segregation, backup/restore and breach notification procedures before external activation.

## Product experience (desktop + web + SDK)

One unified professional console, not 16 separate apps: Overview; Money movement; Customers; Billing; Merchant commerce; Shared funds and approvals; Stablecoin rail (disabled until configured); Reconciliation and cases; Developers; Settings and eligibility. Every transaction detail page shows purpose -> authorization -> attempts -> verified provider observations -> ledger -> settlement -> accounting recognition -> reversals with signed/exportable evidence.

Local-first desktop is an operator view, *not* independent production financial truth. Use read-only/sample mode offline and authenticated server-backed operations for shared/live money. Keep Windows and Arabic RTL existing coverage; expand platforms only when qualified. Provide explicit Sandbox/Shadow/Live labels on every surface and prevent accidental live operations.

## Suggested success metrics, NOT current achievements

Safety: zero duplicate economic effect in fault matrix; exact restore digest parity; all UNKNOWN exposures assigned and recoverable; 100% qualified connector coverage of accepted money transitions.
Developer: >=80% of ten unfamiliar developers complete core sandbox under five minutes; one complete copy-pasteable example for each supported flow.
Pilot: three shadow partners using reconciliation weekly; explain a specific fee/settlement discrepancy within two minutes; no automatic posting from untrusted imports.
Commercial: weekly returned users of reconciliation before broadening suite; auditable unit economics per transaction and processor; test willingness to pay for hosted operations and support.
Stablecoin/commerce: zero live claims until separately certified; one demonstrable sandbox transfer/order lifecycle with deterministic recovery.

## Research citations and future refresh

Primary product/API references:
- Stripe product catalog and Connect: https://stripe.com/products , https://stripe.com/connect
- Circle platform and CPN managed payments: https://www.circle.com/developer , https://www.circle.com/cpn/managed-payments
- Circle CCTP migration/current docs: https://www.circle.com/cross-chain-transfer-protocol
- Square commerce and API: https://developer.squareup.com/docs/commerce , https://developer.squareup.com/reference/square
- Cash App Pay approved partner requirement: https://developers.cash.app/cash-app-pay-partner-api/guides/partnerships/partner-with-cash-app-pay

Future verification checklist: vendor country/account eligibility; all repo revision pins and exact source licenses; actively maintained/fuzzed adapter; provider auth/webhook verification; signed build dependencies; code/data flow PCI impact; conservative financial recovery; runtime benchmarks measured on CoFi's target infrastructure. No live transactions, code imports, test execution or external provider registration were performed by this documentation-only research.
