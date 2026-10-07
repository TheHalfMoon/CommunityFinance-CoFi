# Agent finance

An agent may request a financial action, inspect permitted evidence and simulate policy. The deterministic service owns authorization. No model output, MCP session, natural-language approval or merchant metadata grants spending authority. The same rules protect humans, services and agents; agent-specific tooling exposes their provenance and delegation limits.

Stripe already offers MCP, agent keys, bounded shared payment tokens and machine payment flows. CoFi's opportunity is portable policy and shared-budget enforcement across providers, with evidence that survives provider migration. MCP alone is neither a moat nor a security boundary; see [current Stripe comparison](../research/STRIPE_GAP_ANALYSIS.md).

## Authority objects

Separate authenticated principal, authenticated session, capability grant, economic proposal, approval decision, budget reservation, execution authorization and provider credential handle. An agent session only identifies a requester. A grant binds issuer, subject, organization/project/environment, permitted action/flow, funds/accounts, currency, per-operation and aggregate limits, time window, beneficiary constraints, approval policy, delegation allowance/depth, policy version and revocation epoch. Limit unknown fields and canonicalize the signed representation using a versioned established serialization rule; use standard reviewed signing libraries and domain-separated audiences, never invented cryptography.

Financial limits are exact integer amounts per currency; no global budget achieved by silently converting currencies. If FX is permitted, bind an approved quote and reserve each relevant currency exposure. Beneficiary restrictions use verified identifiers and provider account bindings, not a display name or untrusted metadata. Merchant category checks are enforceable only when qualified evidence is actually available before dispatch; a missing required category fails closed. Category labels supplied by a model are insufficient.

Delegation must prove a subset of the parent's actions, scopes, currencies, beneficiaries, period, amount and depth, with a live valid ancestry chain. Signing a child token is not enough. Parent and children share the parent's aggregate budget; atomically lock/reserve the entire affected ancestor chain in stable order. Check live grant/role/epoch, existing reservations and budget consumption inside the same database transaction that creates authority. Avoid unlimited per-child copies of one allowance. A role removal or parent revocation invalidates descendants for new dispatch.

Approval binds the immutable proposal digest: action, amount/currency, beneficiary, fund, fee/slippage ceiling, allowed provider set, policy revision, expiry and scope. Any material change requires a new proposal/approval. Pricing/routing may choose only within the approved envelope. Separate proposer, approvers and executor where policy requires it. Quorum uses distinct currently eligible principals, not replayed duplicate signatures. A model can summarize a proposal; the authenticated approval interface must display the canonical structured facts.

## From proposal to safe dispatch

1. Authenticate principal/session and enforce object scope.
2. Validate proposal and simulate policy with an explicit data/version watermark. Simulation creates no spend authority.
3. Obtain required human/multisignature approvals through authorized channels; persist evidence.
4. In a serializable transaction, check live grant ancestry/epoch and quorum, reserve the budget, create immutable authorization, intent and queued command, audit the decision, commit.
5. Worker rechecks live revocation/freeze/expiry and authority envelope before atomically marking the attempt DISPATCHING, reserving a stable provider request key and recording its fencing epoch. This commit is the dispatch authorization linearization point.
6. Execute outside the transaction. A network ambiguity becomes UNKNOWN and retains the reservation until evidence resolves exposure. No provider fallback or renewed request key follows a timeout.
7. Verified observations produce domain transitions and journal effects. Release unused reservation only on proof that no exposure remains, or convert it to consumption on confirmed effect. Operator override cannot erase unresolved exposure.

Revocation stops future dispatch and cancels queued commands; it cannot guarantee stopping an operation already authorized at the DISPATCHING boundary. Document the race and attempt cancellation only where the connector supports qualified cancellation/readback. Lease expiry cannot create a second independent execution authority. A lost worker is reconciled, not automatically given a new payment.

API keys are short-scoped credentials, stored as hashes with prefixes, rotation/revocation and last-use evidence. They do not contain provider secrets. Use OAuth/OIDC for interactive identity and workload identity for agents/services; bind session audience/environment. High-risk key/grant creation uses separate privileged approval. Break-glass can freeze/revoke and open a case; increasing spend limits or fabricating approval is prohibited.

## Agent developer experience

Proposed SDK/MCP tools: `agent.proposeSpend`, `policy.simulate`, `proposals.get`, `attempts.get`, `budgets.get`, `reconciliation.explain` and `grants.describe`. Return a machine-readable allow/deny/needs_approval outcome with rule IDs, checked limit values, remaining budget, expiry and proposal digest. No tool called `approve` accepts a model assertion as an approver. A human approval may be presented through MCP only with a separate authenticated principal and the same approval policy.

Use deterministic sandbox scenarios for successful bounded spend, exhausted parent budget, revoked child, expired token, changed beneficiary, duplicate invocation, malicious invoice attachment, prompt-injected merchant metadata, UNKNOWN provider acceptance and policy revision change. Redact sensitive evidence to the requesting role. Finance descriptions returned to an agent are quoted data; they cannot modify the system's financial instructions or grant configuration.

Machine-to-machine payments are an optional execution adapter. Shared payment tokens/MPP or regional rails retain their provider restrictions and consent model. CoFi authority precedes creation/use of an external bounded token, and external revocation/expiry is recorded. A provider's token cap does not replace CoFi's aggregate budget or governance. Do not add a blockchain or store bearer payment tokens in public audit records.

## Source guidance and qualification

Kernux's secret-broker/egress contracts, Ecra's evidence/capability concepts, Deskal's OAuth/action proposal boundary, Orcel's session/idempotency fixtures and Ascout's UNKNOWN handling are selective inputs. None is a qualified financial authorization engine. Ecra has no root license in the inspected revision; use a contract concept only until unit rights are established. Ascout's nonempty reconciliation string must be replaced by verified typed provider/account/operation evidence. Agent runtime orchestration stays external; CoFi owns the financial policy service.

Required evidence before live autonomous spend: concurrent parent/child budget tests, grant attenuation proofs, revocation/dispatch race tests, token replay/audience/scope tests, approval mutation rejection, provider UNKNOWN recovery, credential egress tests and an operational freeze drill. A claimed success rate against prompt injection cannot substitute for these deterministic boundaries.
