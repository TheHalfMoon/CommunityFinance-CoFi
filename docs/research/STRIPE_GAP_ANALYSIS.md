# Current Stripe gap analysis

As of 2026-10-07. CoFi baseline is the source revision in [live state](LIVE_STATE.md), not the proposed architecture. Priorities classify importance to CoFi; they do not assert a shipping date. "Domain" means in-process primitives, not a public production API. Availability requires country/account/contract/release-phase verification.

## Material changes to the competitive premise

The current [usage-based billing documentation](https://docs.stripe.com/billing/usage-based) recommends **Metronome, a Stripe product**, for new integrations. Comparing CoFi only with older Stripe meters would understate the competitor. [Stripe MCP](https://docs.stripe.com/mcp) documents OAuth, environment permissions, agent API keys, session revocation and connected-account restrictions. [Shared payment tokens](https://docs.stripe.com/agentic-commerce/concepts/shared-payment-tokens) include usage/expiration limits. [Machine Payments Protocol integration](https://docs.stripe.com/payments/machine/mpp) supports SPT and eligible stablecoin flows. Agent tooling and bounded payment credentials are not empty competitive space.

[Radar multiprocessor](https://docs.stripe.com/radar/multiprocessor) evaluates payments beyond Stripe. [Treasury](https://stripe.com/treasury) now presents business banking/payment operations and agent automation within set limits. Current API reference reported `2026-09-30.endive`; preview contracts have separate versions. Version pins must be checked again during adapter implementation.

Stripe also publicly lists Managed Payments, Global Payouts and broad payment-method coverage. These increase its integration and liability advantages. None means every product is generally available to every merchant, including a Saudi merchant. CoFi must publish provider/geography matrices instead of a blanket global claim.

## Capability matrix

Source keys: P = [Payments](https://stripe.com/payments); B = [Billing](https://stripe.com/billing); C = [Connect](https://stripe.com/connect); A = [API essentials](https://docs.stripe.com/api/idempotent_requests); W = [Workbench](https://docs.stripe.com/workbench). Other rows link their product evidence. "Build" preserves/extends CoFi ownership; "reuse" requires the source qualification plan.

| Capability | Current Stripe state / evidence | Current CoFi state | Priority | Build / reuse / provider | Differentiation opportunity |
| --- | --- | --- | --- | --- | --- |
| Payments | P: PaymentIntents and managed network execution | Capture accounting; no execution API | MUST HAVE | CoFi intents + qualified connector reuse | One history across provider accounts |
| Elements | P: embedded collection components | Absent | SHOULD HAVE | Provider/vault fields; selectively adapt checkout UI | Replaceable tokenization with explicit limits |
| Checkout | P: prebuilt hosted payment UI | Absent | MUST HAVE for first payment release | Provider-hosted first, CoFi session wrapper | Same business intent across providers |
| Payment Links | P: shareable no-code checkout | Absent | SHOULD HAVE | Common session over qualified checkout | Governed purpose/beneficiary lineage |
| Billing | B: subscription/invoice/collection suite | Deterministic domain pieces | MUST HAVE | Extend existing Rust; reuse fixtures/contracts | Pricing-to-journal transparency |
| Subscriptions | B: recurring lifecycle and recovery | Time-bound domain and authorization | MUST HAVE | Build durable schedules over existing crates | Portable contracts; no processor ownership |
| Invoicing | B: invoice delivery, collection and operations | Draft/finalization/accounting domain | MUST HAVE | Build canonical owner; rendering library/provider | Immutable evidence for each finalized amount |
| Usage-based billing / Metronome | New integrations recommended to Metronome; real-time pricing/credits/contracts | Unit rating/included units; no full credit engine | MUST HAVE for SaaS wedge | Extend deterministic engine; OpenMeter contracts/reference | Open immutable pricing and portable usage receipts |
| Connect | C: onboarding, split movement, embedded surfaces and provider compliance | Funds/distribution/governance domain | MUST HAVE for platform release | CoFi claims/policy; licensed provider onboarding/execution | Governed multi-party approval across providers |
| Tax | [Tax](https://stripe.com/tax): calculation/registration/filing tools | Absent | PARTNER/PROVIDER | Versioned calculation/evidence interface | Retain calculation provenance after provider switch |
| Radar | Multiprocessor risk signals and managed risk options | No production risk engine | PARTNER/PROVIDER | Risk orchestration; deterministic fallback policy | Consistent risk evidence before governed actions |
| Identity | [Identity](https://stripe.com/identity): online verification | Absent | PARTNER/PROVIDER | KYC/KYB/identity adapters; provider eligibility | Reusable onboarding evidence references with consent |
| Financial Connections | [Product](https://stripe.com/financial-connections): consented bank account data/verification | Absent | PARTNER/PROVIDER | Open banking/bank data adapters | Provider-neutral bank matching and consent lineage |
| Treasury | Business financial accounts/movement; provider eligibility applies | No bank/custody service | PARTNER/PROVIDER | Provider-owned accounts, CoFi accounting/control | Own policy/history while bank relationship remains explicit |
| Issuing | [Issuing](https://stripe.com/issuing): virtual/physical card programs | Absent | PARTNER/PROVIDER | Sponsor/issuer processor integration later | Bind card spend envelopes to community/agent authority |
| Capital | [Capital](https://stripe.com/capital): financing services | Absent | NOT CORE TO COFI | Licensed financing partner only on demand | No loan underwriting/balance-sheet moat assumed |
| Revenue Recognition | [Product](https://stripe.com/revenue-recognition): accrual/recognition automation | Receivable/revenue entries, no full ASC 606/IFRS 15 suite | SHOULD HAVE | Explicit recognition schedules + accounting/provider review | Full evidence links; do not claim standards compliance from journal balance |
| Sigma | [Sigma](https://stripe.com/sigma): SQL/AI analysis of financial data | Absent | SHOULD HAVE | Derived queries/read-only analytics | Cross-provider costs and governance flows |
| Data Pipeline | [Product](https://stripe.com/data-pipeline): warehouse/storage sync | Absent | SHOULD HAVE | Versioned export/CDC after durable core | Customer-owned portable history |
| Atlas | [Atlas](https://stripe.com/atlas): incorporation/equity/filing workflow | Absent | NOT CORE TO COFI | Referral/integration if demanded | Avoid distracting legal-product scope |
| Terminal | [Terminal](https://stripe.com/terminal): readers and Tap to Pay | Absent | PARTNER/PROVIDER | Hardware/acquiring provider later | Unified audit only when operator demand exists |
| Climate | [Climate](https://stripe.com/climate): carbon-removal contributions/orders | Absent | NOT CORE TO COFI | Optional external program | Governance-native contribution approvals, no proprietary carbon inventory |
| Link / accelerated checkout | Wallet/account-based checkout acceleration; current product navigation also lists Onelink | Absent | PARTNER/PROVIDER | Provider credential/consent bridge | Do not rebuild a consumer wallet network |
| Customer portal | B: self-service subscriptions/payment details | Absent | MUST HAVE for SaaS release | Build narrow CoFi portal + provider-hosted payment update | Same contracts despite provider replacement |
| Disputes | P/C: cases/evidence/deadlines and risk/liability controls | No acquiring dispute workflow | MUST HAVE before relevant live methods | Selective mappings/fixtures, CoFi state/accounting | Explain principal/fee/reserve across split liability |
| Refunds | P: full/partial refund APIs | Capture/payout accounting without execution refund suite | MUST HAVE | Qualified connector + own refund ceilings/postings | Append-only original-to-return lineage |
| Payouts | P/C and Global Payouts: schedules, recipient movement and options | Payout accounting and disbursement lifecycle | MUST HAVE for outgoing release | CoFi authority, provider custody/execution | Bound approved spend through settlement |
| Payment methods / APMs | P: broad advertised methods with regional eligibility | Three-letter currency type, no connector coverage | MUST HAVE bounded subset | Reuse selected Hyperswitch flows | Honest per-provider method compatibility |
| Mandates/off-session | P/B: consent and recurring execution flows | Subscription lineage, no qualified mandate executor | MUST HAVE for recurring live collection | Provider consent/authentication; CoFi schedule/policy | Preserve consent and actor authority separately |
| Webhooks/events | A/W: event destinations, verification, delivery tooling | No durable public event delivery | MUST HAVE | Own inbox/outbox; reuse verification mappings | Provider event -> unique canonical effect -> CoFi event |
| API versioning | A: monthly nonbreaking/major releases, current Endive reference | No public API | MUST HAVE | Stable CoFi OpenAPI/date versions | Portable versioned state, no silent semantic upgrades |
| Test mode/sandboxes | Official testing docs: simulated integrations | Domain tests, no network sandbox runtime | MUST HAVE | CoFi isolated environment + fake provider | Replay ambiguity and crashes locally |
| Test clocks | B/testing: time-based subscription simulation | Explicit timestamps in domain | MUST HAVE for billing release | Injected deterministic scheduler clocks | Full price/policy/time replay |
| Idempotency | A: client idempotency contracts | Exact journal and business-key replay in memory | MUST HAVE | Durable multi-layer CoFi keys | Restart-safe economic identity independent of PSP TTL |
| API keys | A/MCP: account/API credential models | No service authentication | MUST HAVE | Hashed scoped keys, live revocation | Scope includes financial authority and environment |
| Restricted/agent keys | MCP docs: narrow API access and agent keys | No runtime scoped grant service | MUST HAVE for agents | Deterministic grants and budget admission | Exact intent/beneficiary binding, aggregate spend ceilings |
| SDKs | A: major server/client languages | Rust domain crates only | MUST HAVE TS/Python initially | Generate from OpenAPI, handwritten ergonomics | One model for SaaS/community/agents |
| CLI | Official developer docs: local tooling and events | No CoFi CLI | MUST HAVE | Build `dev/listen/trigger/doctor` | Reproducible complete local money workflow |
| Developer dashboard | W: logs, tooling, account control | Desktop informational shell/site | MUST HAVE narrow operations | Extend CoFi UI using application service | Single payment/approval/journal/reconciliation timeline |
| Workbench | W: shell/API explorer, destinations and health | Absent | SHOULD HAVE | Focus on incident inspector and replay | Financial causes alongside network trace |
| Observability | W/P: operational health and request/event inspection | Audit domain, no deployed operations | MUST HAVE | Structured traces/metrics/redaction and canonical references | "What happened?" includes accounting/policy causes |
| MCP / agentic commerce | MCP OAuth/sessions, agent keys, commerce suite and SPTs | No agent transport or budget runtime | COFI CAN DIFFERENTIATE | MCP/OpenAPI over same CoFi authority | Organization/community policy spans providers and spend types |
| AI agent payments | Bound credentials/SPTs and agent tools exist | Governance precursor only | COFI CAN DIFFERENTIATE | Own delegated authority, provider tokens | Joint budgets/quorum/lineage across agents and humans |
| Machine payments | MPP via PaymentIntents and eligible stablecoin/SPT flows | Absent | SHOULD HAVE later | Protocol adapter over qualified execution | HTTP payment challenge cannot bypass budget/receipt authority |
| Connect onboarding/compliance | C: provider KYC/sanctions/PCI and risk allocation | No licensed onboarding operation | PARTNER/PROVIDER | Replaceable provider contract and state | Portable status evidence; provider acceptance still required |
| Global acquiring/payment reach | P/C: broad network and regional partnerships | No acquiring licenses/network | PARTNER/PROVIDER | Qualified licensed providers | Geographic orchestration without claiming licenses |
| Managed Payments / merchant of record | Public current product listing advertises MoR solution | Absent | PARTNER/PROVIDER | MoR partner if business model needs it | Do not confuse own billing with liability transfer |
| Shared treasury/quorum | Financial controls exist; current reviewed sources do not establish CoFi-style community quorum as a general primitive | Existing proposal/quorum/fund domain | COFI CAN DIFFERENTIATE | Extend existing CoFi | Native collective authority connected to actual execution |
| Self-hosted canonical financial state | Reviewed services are hosted Stripe products | Open pure Rust; production persistence missing | COFI CAN DIFFERENTIATE | Own durable service/export/restore | Correctness without mandatory hosted control plane |
| Portable financial history | APIs/exports exist; provider migration still constrained | Domain lineage; no complete durable export | COFI CAN DIFFERENTIATE | Export verifier and migration product | Reconstruct obligations/authority beyond one provider's objects |

## Migration compatibility decision

Do not advertise "change the base URL and all Stripe works". PaymentIntents look familiar, but payment-method tokens, 3DS, Connect liability, pagination, webhook event versions, error semantics and retry rules differ. A thin optional compatibility surface may support create/retrieve/confirm/capture/cancel/refund for a qualified subset and a fixed Stripe API version. Explicitly reject unsupported fields/modes, supply golden semantic fixtures, publish migration gaps, and require token/provider portability checks. Core CoFi objects remain independent.

For a founder choosing vendors: Stripe wins turnkey reliability, distribution, hosted UI, risk data, regulatory breadth and operational responsibility. CoFi wins only when verifiable control, governance and portable history justify operating another layer. CoFi should often use Stripe underneath; paying Stripe for execution is compatible with owning economic state.

## Other named alternatives

Adyen is a network/acquiring and enterprise platform choice, not merely software to clone. Paddle's merchant-of-record proposition transfers responsibilities CoFi alone cannot transfer. Chargebee offers mature managed billing across gateways; processor neutrality alone does not beat it. Lago, Meteroid and OpenMeter already address self-hosted/usage monetization; Hyperswitch addresses connector/routing breadth. A collection of these tools may be superior initially if it has established operational owners.

CoFi must reduce the integration seam between these capabilities: contract -> authority -> external attempt -> posting -> settlement -> explanation. Benchmark total implementation/operation time and migration fidelity before claiming superiority. Competitor marketing is not evidence of provider captivity or a guaranteed cost advantage; validate acquisition/product claims through primary sources before using them in positioning.
