# CoFi north star

## What CoFi is

CoFi is open financial infrastructure that lets applications own their economic history and financial authority while buying payment execution from replaceable providers. Its durable ledger records economic effects; its policies determine who may cause them; its adapters observe outside execution; its reconciliation explains differences. Billing, community funds and agent spending are applications of that same model.

The promise is: **understand, authorize, execute and reconstruct every money flow without surrendering its history to a processor**. It must be possible to run the money plane without a mandatory CoFi-hosted service, export complete state, and replace a provider without inventing a second financial truth.

"Open Financial Operating System" is a useful internal long-term description, but a weak launch promise. It implies an enormous suite, makes custody sound native, and gives a developer no immediate task to accomplish. Launch around portable financial control, with a concrete quickstart and one coherent API. Do not create sixteen separately marketed products before there is one reliable end-to-end workflow.

## Who chooses it first

Start with developer-led SaaS and small platforms that combine usage revenue, vendor spend and revenue distribution, already using a licensed PSP. They suffer from fragmented invoices, processor balances, application credits and approval spreadsheets. They can adopt shadow reconciliation before changing their checkout or subscriptions. Community/cooperative workflows provide an adjacent proof of the same governance primitives. Enterprise routing is a later expansion requiring stronger operations and wider qualification.

An ordinary single-processor store that values a hosted checkout and instant operational support more than portability should usually choose Stripe, Paddle or its commerce platform today. CoFi has to earn the switch through recovered engineering time, fewer unexplained balances, stronger spending controls or demonstrable migration flexibility. It cannot initially match a global acquiring network or merchant-of-record liability transfer.

## Structural advantage, with falsifiable limits

| Advantage | User benefit | What makes it real | What would falsify it |
| --- | --- | --- | --- |
| CoFi-owned history | Explain cash, obligations, fees and beneficiary claims together | Durable lineage from contract to journal to settlement | Ledger cannot reconstruct billing/provider changes |
| Provider neutrality | Negotiate and change execution without rewriting economic logic | Two qualified adapters share one contract; unsupported capabilities fail explicitly | Token portability or provider semantics force hidden lock-in |
| Governance-native money | Budget/quorum/vendor restrictions apply before execution | Authorization bound to exact intent, budget atomically reserved | Approval is merely a dashboard button |
| Agent-safe delegation | Machines act within accountable budgets | Narrow grants, live revocation, deterministic checks and simulation | Generic MCP write access is presented as sufficient authority |
| Portable deployment and data | Self-host, recover and exit | Open core, export verifier, restore drills, no mandatory hosted correctness dependency | Useful self-host requires an unavailable control service |

Stripe already offers agent keys, MCP, bounded shared payment tokens and machine payments. Radar can evaluate other processors. Lago and Chargebee are also processor-neutral. These facts rule out "open source", "multiple PSPs" and "supports agents" as standalone differentiation. CoFi's opportunity is the **composition**: auditable economic history plus organization/community authority spanning providers, billing and outgoing spend.

## Why switch, and why start without switching

Offer an importer that explains an existing processor's settlements and fees, produces explicit reconciliation breaks, and exports a proof bundle. The customer keeps their current checkout. Once this is useful, make new billing contracts and governed payouts originate in CoFi. Then move eligible payment traffic gradually. The proposition is cumulative operational value, not a forced all-at-once migration.

Do not promise lower PSP fees from orchestration alone. Provider contracts, data visibility, volume and card acceptance dominate costs. Report observed savings net of orchestration operating costs and failed-payment losses, against a declared counterfactual. Do not promise universal card token mobility or byte-for-byte Stripe compatibility.

## Why a platform can emerge

The stable nucleus is an economic command/observation/authorization contract, not a catalog of screens. Payment connectors, reconciliation importers, tax/risk/KYC providers, accounting exporters and agent tools can build against it. A connector conformance suite creates reusable trust evidence; replayable incident fixtures help every extension. Stable APIs and provenance-qualified release artifacts give maintainers a reason to invest.

The ecosystem grows only if maintaining an adapter is economically feasible. Publish supported flow matrices, pinned versions, a test processor, deterministic failure cases and contributor ownership rules. Count active maintained extensions and production users, not repository stars or connector filenames. A provider integration cannot inherit certification solely because its source was copied.

## What CoFi must never become

CoFi must not become a bank by assertion, an unreviewed donor monolith, a second bookkeeping engine beside its own ledger, a universal unbounded scripting runtime, an AI-authorized money bot, a cloud-only product disguised as self-hostable, or a feature checklist that obscures the source of truth. It must not label allocated claims as legally held funds or interpret an authorization as settlement.

Maintain the existing pure Rust safety boundaries, checked integer money, exact replay, scope isolation, community governance and append-only correction. Desktop remains a useful local operations surface, but its current shell is not evidence of complete financial operations. Build real operations by reusing the service boundary rather than constructing a separate desktop ledger.

## Potential moat and its weakness

The strongest potential moat is a growing body of **verified financial continuity**: portable histories, authorization receipts, provider-specific conformance, migration fixtures, reconciliation mappings and operational knowledge, all interoperating through one durable model. Users gain trust and integration depth with each workflow; contributors gain reusable qualification assets; migration into and out of CoFi stays supported.

This is defensible through execution, ecosystem maintenance and accumulated correctness evidence, not exclusivity or trapped data. Competitors can copy the design. Stripe's distribution, licenses, risk data and checkout network remain formidable. If CoFi cannot make its first billing-to-settlement story simpler than assembling tools, the compound moat never forms.

## Success measures

Targets, not observed results: first sandbox payment under five minutes; at least 80% unaided completion across ten unfamiliar developers before advertising that promise; a complete payment incident explained in under two minutes; no duplicate economic effect in the crash matrix; two providers passing the same conformance suite; a restored export producing identical scope journal digests; three pilot customers using reconciliation weekly before widening the launch scope.

Proceed with P0 regardless of market enthusiasm because current durability is inadequate for production financial state. Expand product scope only on measured adoption and qualified operations. See [adoption](ADOPTION_STRATEGY.md), [red team](PLAN_REVIEW.md) and [roadmap](PLATFORM_ROADMAP.md).
