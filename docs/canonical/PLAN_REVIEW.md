# Design scorecard, red team and moat

This is an internal adversarial review of the proposal, not an independent agent report, a security audit or exact-head Jev/OCR qualification. Implementation evidence and external design-partner results remain absent. Scores are judgments on a 0–10 scale, not measurements: 0 absent, 5 a plausible partial foundation, 8 coherent with substantial evidence for the claimed scope, 10 exceptional independently demonstrated maturity.

Separate current product readiness from design confidence. A high design score does not imply shipped production capability. The design can be complete enough for bounded P0 work while broad platform readiness remains low. Publication requires repository review; live activation requires the roadmap's operational/provider/legal gates.

## Twenty dimensions

| Dimension | Current readiness | Design confidence | Gap when below 8; must fix before which work? |
|---|---:|---:|---|
| Financial correctness | 6 | 8 | Existing checked ledger/lineage strong, but no durable all-registry proof. P0 preserves invariants; live money blocked until crash/concurrency/partial-effect conformance. |
| Durability | 1 | 8 | BTreeMap-backed state. G001-G008 are the implementation to repair it; not a reason to block storage coding after review. |
| Payments breadth | 2 | 6 | Accounting/observation domains are not live acquiring. Qualify one complete P3 vertical before breadth; exact backend selection is a bounded experiment. |
| Billing breadth | 4 | 7 | Existing unit price/included usage; tiers/credits/tax/late/proration not complete. Resolve contract semantics before each P4 extension, not before P0. |
| Community finance | 5 | 8 | Existing governance/domain strengths but no complete persisted operating UX. Two-cycle pilots and provider/legal liability proof before broad launch. |
| Platform/marketplace support | 3 | 7 | Internal allocations do not replace onboarding/custody/Connect. Provider contract/liability/negative-balance rules before P5 live platform cohort. |
| Agent readiness | 2 | 8 | Authorization lineage useful, no shipped live grants/MCP service. Budget/revocation/ancestry tests before P1 authority claim and P5 autonomous spend. |
| Developer experience | 2 | 7 | No public financial HTTP API/SDK/dev CLI today. External timed quickstart and two coherent SDKs before P1 exit. |
| Self-hosting | 2 | 7 | Domain/library and unsigned Desktop, no qualified server stack. Packaged startup/resource and restore/upgrade drills before production self-host support. |
| Provider neutrality | 6 | 8 | Neutral existing contracts; connector selection/settlement portability unproven. Two qualified providers and migration evidence before claiming execution neutrality. |
| Migration experience | 1 | 6 | No executable import/shadow/cutover/export tool; tokens/mandates may not move. P2 proves one-cycle mapping and P6 provider cutover, no drop-in promise. |
| Security | 4 | 7 | Rust safety is one boundary, not runtime auth/secret/tenant/PCI assurance. Threat controls are explicit; P0-P3 tests and qualified dataflow review precede respective activation. |
| Observability | 3 | 7 | Audit domain exists but no production metrics/cases/witnessing. G007/G008 and P2 need operator-owned lag/UNKNOWN/integrity evidence. |
| Extensibility | 4 | 8 | Contract/conformance/isolated executor design is coherent; no arbitrary native plugins. Qualify registry/import process before external connector promotion. |
| Globalization | 4 | 6 | Seven Desktop locales/RTL do not prove currency exponents, rails, tax or legal coverage. Registry and per-region/provider eligibility before every added market. |
| Documentation | 5 | 8 | This package gives architecture/decisions/provenance/grains; runnable API examples still future. P1 docs must be executable against released versions. |
| Enterprise readiness | 2 | 6 | No demonstrated SSO/residency/HA/DR/SLO/support operations. P7 evidence/procurement required; do not promise enterprise readiness at P0. |
| Open-source attractiveness | 6 | 8 | Apache core and original Rust are useful; sustainable maintainers/qualified imports missing. Preserve legal transparency, safety/export openness and small grains from first implementation. |
| Operational simplicity | 3 | 7 | Proposed one PostgreSQL stack is plausible, but replay/sidecar costs unmeasured. G008 bounds and P3 experiment determine supported resources; no Redis/Kafka default. |
| Differentiation from Stripe | 4 | 7 | Portable combined authority/history is a hypothesis; Stripe agent/multiprocessor/billing breadth is real. P2/P5/P6 must prove material benefit; narrow or stop if it does not. |

No numeric average is used to obscure critical gaps. Durability, unsafe dispatch, tenant isolation and missing authority cannot be offset by documentation or connector count. Before **implementation**, adopt the coherent P0 scope, preserve live governance and choose bounded storage/dependency details. Before **production**, satisfy the relevant empirical qualification gates; documentation cannot make those scores 8.

## Adversarial questions and repairs

| Challenge | Honest answer | Repair incorporated in the plan |
|---|---|---|
| Why would a developer still prefer Stripe? | Simpler integration, trusted operations, acquiring/checkout/tax/MoR and broad current tooling | CoFi enters beside Stripe via shadow control; does not rebuild its service coverage or require checkout replacement |
| Why would a company refuse to migrate? | Tokens, mandates, subscriptions, legal contracts, accumulated ops trust and migration risk | Source-ID imports, frozen issued invoices, one complete shadow cycle, new-write cohorts, explicit nonportable objects and rollback that never erases money |
| Is CoFi doing too much? | The full internet financial OS is too broad for an early team | One beachhead/one complete vertical, minimal runtime stack, partner-led regulated functions and demand-based expansion |
| Is community finance compelling? | For some shared-budget groups; ordinary SaaS may not need voting | Three groups/two cycles, usable approval/evidence UX; keep governance optional at API edge while money invariants remain shared |
| Are agents a gimmick? | MCP and agent keys are increasingly commodity | Same deterministic shared-budget/revocation model for services and humans; prove bounded autonomy, do not build a new agent runtime |
| Does orchestration become commodity? | Likely, and incumbents already offer multi-provider tools | Reuse mappings, compete on verified economic/authority continuity and operations, not connector count |
| Is a ledger useful to application developers? | Only if it reduces reconciliation/credit/budget code; raw debits/credits alone are burdensome | Domain APIs choose canonical postings and expose explainable trace, examples and export instead of requiring callers to be accountants |
| What creates regulatory risk? | Custody, onboarding control, payouts, redeemable balances, pooled investments, tax/issuing/lending roles | Explicit legal/provider activation gate; one legal operator first; no claims of exemption or worldwide coverage |
| Which subsystems are expensive to operate? | Vault/PCI, fraud networks, worldwide custody, queues/analytics sprawl and per-provider support | Hosted tokens, partner services, one Pg stack, few owned connectors and explicit operational cost qualification |
| Where will self-host users struggle? | Secrets, backups, upgrades, UNKNOWN outcomes, large-scope replay and provider eligibility | Doctor/restore quarantine, packaged dev stack, supported operating bounds and managed option; no silent live mode |
| What creates ecosystem lock-in? | Opaque schemas, nonportable tokens, proprietary policy and unmaintained plugins | Versioned canonical exports/contracts/conformance, honest provider restrictions, open safety/export core and maintained import records |
| What kills adoption? | A large source import with poor UX, incorrect status, no trust/support, no demonstrated use case | P0-P2 correctness/quickstart/shadow pilots before broad launch; measured benefit and stopping criteria |
| What makes CoFi irrelevant in three years? | Incumbents supply portable verified shared-authority history with better DX/ops, or users do not value it | Repeat competitive research and pilot evidence; become a smaller durable control/core offering if thesis fails |
| Which Stripe features should not be copied? | Acquiring network, Link network, Terminal hardware, Atlas services, tax filing/registrations, bank sponsorship, issuing/lending and carbon procurement | Provider integrations/references, not local feature-count competition; see detailed priority matrix |

## Failure scenarios used to check coherence

1. Payment accepted; response lost; worker crashes; lease expires: DISPATCHING -> UNKNOWN, reservation retained, same provider identity read back, no fallback/new key. This is consistent in storage, state, connector, agent and roadmap documents.
2. Database COMMIT succeeds; HTTP acknowledgment lost: durable result resolves by same operation identity. Reconstruct all registries/authority/replay keys, not only balance rows.
3. Child agents each request the parent's full allowance concurrently: live ancestry and ordered ancestor budget locks admit only the allowed aggregate exposure.
4. Proposal approved; beneficiary/amount changes: immutable digest differs and approval no longer applies. A dashboard edit cannot grant authority.
5. Backup predates an external payment and revocation: restored dispatch stays disabled, external outcomes and authority reconciled before replay; no blind queued-command resend.
6. Settlement webhook contradicts fee/amount/readback: preserve raw verified evidence, open a case/suspense under policy, do not overwrite history or fabricate fee/cash.
7. Donor connector adds a dependency or status: new pin is pending qualification, old qualified mapping still resolves historical attempts; no inherited production readiness.
8. Community contributor paid, then customer refund/chargeback arrives: append liability/receivable/reserve use under selected rule, no deletion of payout history.

These checks repaired the main risks: generic outbox semantics were separated from payment dispatch; restored state was prevented from replaying external effects; parent budgets became shared rather than cloned; provider neutrality stopped claiming universal token portability; billing finalization/late corrections became immutable; source licenses stopped inheriting permission across the ecosystem.

## Potential moat and falsification

The strongest potential moat is **verified financial continuity**: a maintained conformance corpus and migration/recovery tooling that preserve an application's economic history and authority constraints across providers, with operating evidence from real recurring/shared-budget workflows. Ledger + billing + governance + agents can generate difficult boundary cases and reusable assurance; merely bundling their features is not defensible.

This advantage compounds through trusted maintainers, qualified connectors, worked migration/restore cases and developer confidence. The history remains exportable: defensibility comes from better continuity and assurance, not trapping customer data. There is no network-effect claim from a few source files. Open source enables inspection/adoption but is not itself the moat.

Falsify it with pilot evidence: no meaningful reduction in unexplained deltas/operator work, no demand for shared portable authority, equivalent incumbent continuity at lower total cost, or inability to maintain qualification economically. If any persist, narrow to the durable control/core or interoperability product. The plan is internally coherent enough to propose bounded storage work after review; its commercial moat and production readiness remain hypotheses to be tested.
