# Architecture and product decision register

Status: proposed by this master plan, subject to exact-head review/adoption. Rationale and falsification conditions make these revisable decisions rather than implementation claims.

| ID | Decision | Rationale / alternatives / reconsider when |
|---|---|---|
| D01 | Portable financial control layer; SaaS/platform shadow entry | Broad financial OS is a long-term description, not a launch product. Reconsider if pilots value only a dashboard or independent durable core. |
| D02 | Preserve all 21 original domain crates and current Desktop identity | Existing deterministic accounting/lineage outweigh donor feature count. Extract a new boundary only with measured dependency/ownership evidence. |
| D03 | PostgreSQL canonical durable state; pure Rust domain decides legality | Strong atomic/recovery tools without a second money engine. Do not add SQLite financial truth or donor store; revisit deployment variants only with equivalent conformance. |
| D04 | One atomic unit of work across ledger, registries, authority, replay, audit and outbox | Journal-only durability loses the authority/economic context. Full reference replay first; optimize verified snapshots/selective hydration only after dependency proofs and benchmarks. |
| D05 | SERIALIZABLE plus stable locks and bounded DB-only retries | Prevent lost updates and budget races. No provider network inside transaction. Reconsider granularity, not financial invariants, if throughput measurements require it. |
| D06 | UNKNOWN is a first-class held-exposure state | Timeout/lease expiry does not prove rejection. Query/qualified exact resend only; never fresh attempt/fallback until evidence. |
| D07 | Minimal new storage/application/service crates; API/worker/events/recovery as modules first | Avoid crate/service explosion. Split when independent runtime/dependencies or teams justify it. |
| D08 | Hyperswitch selective port or bounded executor sidecar; Prism qualification experiment | Reuse mature mappings without competing truth. Reject sidecar if hidden retry/routing cannot be disabled; choose based on measured closure and conformance. |
| D09 | No AGPL/GPL source merge into Apache core | Main Hyperswitch permission does not extend to Decision Engine, Lago, Meteroid or Vendure. Missing-license founder units remain reference-only; per-unit rights can change a future decision. |
| D10 | Hosted/tokenized checkout first; vault/PCI infrastructure optional separately qualified | Safe usable UX before broad card-data operations. A customer with a qualified need/assessor-supported design may justify an optional vault. |
| D11 | Shared human/service/agent authority with parent budget reservation and live revocation | Agent session and signed ancestry alone are insufficient. Models propose/simulate; deterministic service authorizes. |
| D12 | Provider observations never directly become caller-selected accounting | CoFi verifies linkage and applies approved economic rules. Import unknown facts into cases/suspense, not fabricated balances. |
| D13 | Provider-led custody/KYC/tax/issuing/treasury/MoR/legal services | Software does not reproduce regulated coverage. A new legal role requires separate approval/qualification, not just a feature flag. |
| D14 | Exportable history/contracts; bounded Stripe compatibility | No base-URL-only migration promise for tokens, mandates, Connect or tax. Reconsider compatible subset only with exact fixtures and supported provider versions. |
| D15 | Core safety/export stay open; managed/support/qualified integrations fund operations | Open source alone is not a moat. Pricing/business model remains a pilot hypothesis, not a constraint that weakens safety. |
| D16 | Normal merge, identity and genuine exact-diff review remain | Evidence follows exact base/head; delegation/workflow success is not automatically semantic review. No force/squash/rebase/history rewriting. |

Open decisions are intentionally bounded: exact storage driver/MSRV-compatible version; schema/index locking granularity; sidecar vs port vs qualified Prism; witness/backup platform; first supported rail/region/provider account; pilot price/support hours and actual performance/SLOs. Resolve each before its grain or activation gate, not by pretending documentation established runtime evidence.
