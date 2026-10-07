# Billing, pricing and entitlements

Preserve `cofi-metering`, `cofi-rating`, `cofi-subscriptions`, `cofi-invoicing`, `cofi-billing` and their authorization crates. The inspected rate plan supports a checked unit price and included units, not a complete enterprise pricing system. Expand the deterministic domain incrementally; external products supply patterns, optional collection and tax facts, never a competing canonical invoice ledger.

## Economic pipeline

Usage receipt -> validated immutable usage event -> versioned aggregation -> rating explanation -> draft invoice -> authorized finalization -> receivable posting -> payment application -> settlement reconciliation. Entitlement admission and spend reservation are separate from eventual invoicing. A usage system can be delayed; it cannot authorize unlimited spending while the budget projection catches up.

Events carry source event ID, tenant/environment, subject, meter/version, integer quantity or explicitly bounded rational quantity, occurrence/receipt times, dimensions and evidence hash. Duplicate source IDs with changed content conflict. Accept only declared dimensions, bounded cardinality and size. Raw prompts, model responses and personal content are unnecessary billing data. For AI workloads, meter input/output/cached tokens, model/version, requests, tool invocations, GPU time and storage in explicit units; preserve provider usage evidence separately from customer billable rules. Cost forecasts are estimates, not postings.

Aggregation policies define SUM, COUNT, LAST and MAX only where semantics are clear; deduplication occurs before aggregation. Specify interval boundary inclusivity, named time zone, DST handling, late-event grace window and event correction linkage. Meter version changes start a new explicitly selected interval. Late usage after finalization becomes a documented next-period adjustment or credit/debit note; never modify an issued invoice invisibly. Provisional dashboards show cutoffs and lag.

## Pricing modes and safe customization

| Model | Required semantics | Order |
|---|---|---|
| Flat recurring and per-seat | Seat snapshot/change timing; effective date and renewal policy | First billing vertical |
| Per-unit usage and included usage | Units, allowance, overage, reset and carry policy | First billing vertical |
| Graduated tiers | Each marginal bracket priced independently; fixed boundary convention | Next |
| Volume tiers | Whole quantity priced by one selected bracket; prohibit accidental graduated interpretation | Next |
| Package/block pricing | Explicit ceiling division, minimum and leftover unit policy | Next |
| Prepaid monetary credit | Liability, purchased amount, expiry/refund law, consumption order | After credit accounting |
| Promotional/noncash credit | Separate ledger/type, no assumption of refundable cash | After credit accounting |
| Committed spend plus overage | Contract schedule, drawdown and true-up explanation | Negotiated enterprise cohort |
| Hybrid base/seats/usage | Deterministic component ordering and one final rounding policy | After component conformance |
| Customer contract override | Immutable negotiated version/approval, effective period and termination | Enterprise cohort |
| Custom formula | Closed typed AST, no arbitrary code/SQL/network | Only demonstrated demand |

Configuration should be JSON/YAML parsed into a typed immutable contract, validated through the same Rust constructors. SDK builders emit that contract. A custom formula is a bounded expression tree with integer/rational arithmetic, allowlisted fields, deterministic functions, static type/range checks, maximum instruction/depth limits and no time, randomness, filesystem or network access. Expose simulation and explanations; reject overflow and unbounded compute before publication. Do not implement an unconstrained scripting language or permit a customer SQL query to become financial authority.

Store prices as integer minor amounts or exact rational unit rates with declared numerator/denominator. Multiply with checked wide arithmetic; declare rounding mode, stage and residual allocation. Round once per declared charge component; allocate residual minor units by stable item ordering and retain the explanation. Tax providers may require different legal rounding; persist the qualified rule/version. Do not use floating point, including donor entitlement reset values. Currency conversion requires an explicit quote, rate representation, provider evidence, expiry and balanced accounting in each currency; no ledger entry balances by adding unrelated currencies.

Subscriptions freeze contract version for the effective interval. Upgrades/downgrades define proration from exact calendar interval boundaries, price versions and already consumed allowances. Billing anchors, trial termination, pause, cancellation, renewal and payment failure are explicit transitions. Collection failure does not retroactively invalidate earned revenue or rewrite usage. Dunning is scheduled policy, bounded attempts and consent-sensitive method selection; UNKNOWN attempts block new collection retries.

## Invoice and credit ownership

Finalization stores invoice lines, usage cutoff/aggregation version, pricing/discount/tax snapshots, parties, legal numbering policy, authorization lineage and journal entry IDs atomically. Issued invoices are immutable. Voids, credit notes and debit notes reference originals and append effects. Tax calculation success is not tax registration or filing. Invoice numbering/retention and e-invoicing integrations must be qualified per jurisdiction before commercial release there.

Paid prepaid balances represent a liability until earned, subject to actual terms and legal obligations. Promotional credits are separate; expiration or transfer rules must not leak into paid balances. Credit notes, cash refunds and provider refund attempts are linked but distinct. Payment allocation cannot exceed outstanding receivables; partial payments and overpayments have explicit account treatment. Refunds do not automatically reopen service entitlements without a policy transition.

Entitlement checks use the authoritative versioned grant and atomically reserve constrained quantities. Admission, consumption, release and expiry are separate events; parent/child budgets cannot be doubled. An offline optimistic admission mode is allowed only for explicitly nonfinancial, noncritical benefits with bounded exposure, never payment execution. Product access can be withheld after failed collection under the selected contract, with a documented grace policy.

## Reuse and interoperability

OpenMeter's meter/entitlement contracts are useful design inputs and a possible optional upstream event collector. CoFi still deduplicates and owns the billed aggregation snapshot. Its float-based values cannot be copied into financial arithmetic. Kill Bill's invoice schedule and payment tests can supply ported fixtures after unit/dependency license checks. Lago/Lago API and Meteroid provide concrete reference behavior for credits, dunning and outbox structure but are AGPL; no source copying into the Apache core is planned. An external billing product may continue issuing legacy invoices during migration; identify which system owns each cohort/period and prevent both from finalizing it.

Qualify with month/year/leap/DST boundaries, huge/small quantities, tier edges, negative corrections, changed duplicate events, concurrent credit consumption, late usage, interrupted finalization, tax timeout, payment UNKNOWN, partial refund and export/replay equality. A table of beautiful prices without these examples is not a billing product.
