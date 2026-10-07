# Community and platform finance

Community finance becomes useful when a group can see why money is available, who may commit it, what was approved and what actually happened. Preserve CoFi's community, governance, spending, disbursement, allocation, transfer and authorization lineage domains. Build a usable operating flow around them rather than an additional social-network or token-governance product.

## Initial product and ownership

First cohort: a developer platform, open-source organization or cooperative with one legal operator, a shared operational fund and an existing licensed payment/payout provider. CoFi supplies policy, internal economic attribution and evidence. A ledger fund is not a bank account or proof of safeguarded custody. Provider settlement, legal ownership, tax and member entitlement must be modeled explicitly.

Product objects include community, member/role, policy version, fund/budget period, proposal, approvals, reservation, authorization, disbursement attempt, provider observation, settlement and reconciliation case. Funds can be earmarked for operations, grants and reserves; their balance and available-to-spend projection are distinct. Budget availability deducts committed reservations, unsettled exposure and protected reserves. Negative balances and deficits are visible, governed and never concealed by deleting commitments.

Proposal states: draft -> submitted -> approval_pending -> approved/rejected/expired -> reserved -> dispatching -> outcome_pending/UNKNOWN -> paid/failed/reconciled. Policy changes, membership changes, altered amount/beneficiary and expired approvals are handled explicitly, not by editing an approved proposal. Current quorum eligibility must be checked at the defined authorization and dispatch boundaries; audit the policy snapshot and live membership/revocation checks. Long-lived approved proposals may require renewal.

Approval matrices support amount thresholds, fund/category, beneficiary risk and role combinations; prevent one identity counting twice through multiple roles. Separation of duties is configurable and mandatory for protected funds. Emergency authority is narrowly allowed to freeze/revoke, not silently transfer or bypass quorum. Recovery from a lost approver requires a predeclared succession policy and auditable cooling/approval procedure. Dispute resolution may append a new decision and financial adjustment; it cannot overwrite historical approvals.

## A concrete shared-fund workflow

A platform receives provider-confirmed customer revenue. CoFi records clearing/receivable effects, then settlement and fees from qualified evidence. A versioned distribution rule allocates eligible net revenue to contributor liabilities, operating funds and reserves. It cannot allocate unsettled estimates as cash. Integer residuals are distributed by a stable documented order, and each allocation has its own replay/business key.

A member proposes a software purchase of `"10000"` SAR minor units from the operating fund, with verified vendor identity, invoice evidence and fee ceiling. Distinct eligible approvers meet the frozen policy. The application transaction reserves the full maximum exposure and creates the authorized vendor disbursement. The provider may accept the transfer while its response is lost: the member sees UNKNOWN, the budget remains reserved and no replacement transfer is submitted. Readback/settlement evidence resolves the attempt; final accounting and audit append exactly once. An unused fee allowance is released only after confirmed resolution.

Reimbursements bind an eligible expense, claimant, evidence and policy; uploading a receipt is not authorization. Grants support milestones, restricted purposes, tranche limits, expiry and accountable beneficiary observations. Partial payout failures preserve completed tranches and reconcile unknown ones separately. Collective AI spending delegates a shared parent allowance under the same quorum/reservation rules in [agent finance](AGENT_FINANCE.md).

## Platform and marketplace extensions

Maintain explicit platform, connected legal party, provider account and economic relationship mappings. Licensed providers handle onboarding/KYC/sanctions checks, connected-account eligibility, custody and movement. CoFi verifies their observations and fails closed if required status is stale/unavailable. Provider balance is observed evidence, not the platform's canonical ledger. Never infer a funds-transfer license from using an API.

Support commissions, revenue splits, contributor payables, reserves, refund/dispute attribution, negative balance handling and payout schedules through approved rule versions. The rule includes who owes refunds/chargebacks, what reserve can be consumed and who may recapitalize a deficit. A refund after a payout may create a receivable from the responsible party; it cannot pretend the payout never occurred. Collection of that receivable is a separate legal and operational flow. Cross-scope transfers use paired balanced workflow entries without weakening the existing CrossScopeEntry invariant.

Account portability is bounded: customer/merchant IDs and exported history can move; provider tokens, legal onboarding, payment mandates and bank sponsorship may not. An account migration requires new provider qualification, consent where required and reconciliation of the old provider's remaining disputes/settlements. Do not market CoFi as a drop-in Connect replacement without those capabilities.

## Transparency, privacy and adoption

Dashboard modes: member budget/requests, approver queue, operator reconciliation and auditor evidence/export. Public transparency uses an explicitly approved redacted projection with pseudonymous beneficiaries and aggregate balances. Private invoices, personal identity, bank details, provider credentials and bearer tokens never appear in a public audit feed. Members see only permitted funds and cases; being in the same community is not universal data access.

The distinctive value is an understandable approval-to-money trace with portable evidence, not more voting widgets. Validate with three operating groups over two budget cycles: can members determine available budget without a spreadsheet, can an auditor trace each payout to authority/evidence, and can operators resolve every simulated ambiguity without duplicate spend? If groups only need a spreadsheet plus bank transfer, reduce the product to exports/approvals rather than insisting on a full platform migration.

Investment clubs, public pooled investing, transferable redeemable balances, lending and yield products are outside the initial product. Their legal/custody/consumer protection requirements are substantive partner and jurisdiction decisions. The same restraint applies to treasury balances and cooperative profit claims; internal accounting does not remove legal obligations.
