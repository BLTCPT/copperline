# Functional parity roadmap

This sequences the remaining gaps in [the capability map](parity.md). Milestones are proposed implementation units, not completed features or a promised parity percentage. Every milestone must leave a runnable native/CLI/API workflow and preserve existing financial, audit and recovery invariants.

## M4 — correctable actuals and reconciliation (implemented local subset)

M4 delivers independently reviewed full invoice reversals/atomic replacements, original-period net accrual recalculation and signed local settlement adjustments with exact source/prior-record checks. See [workflow semantics](corrections-reconciliation.md) and [verification](../evidence/verification.md).

Remaining agreement lifecycle work includes prospective amendments, partial credit allocations, credit-note delivery and accounting-period closure. No bank/payment/ERP posting is part of the local milestone. All older invoices, accruals and recognition records remain immutable.

## M5 — customer contracts and transactional replacement (implemented local subset)

Immutable approved customer contract editions, half-open date/quantity intervals, priority/conflict checks, frozen grid/list origins and historical source reconstruction are implemented. Atomic quote-line replacement recalculates all retained Draft lines and preserves prior approval cycles. See [contract semantics](customer-price-agreements.md).

Multi-customer/segment terms, bulk contract imports and larger-scale matching remain open. Validated backup/restore remains a useful bounded operational follow-up.

## M6 — explicit dated conversions (implemented local subset)

Approved immutable direct FX/product-UOM books, exact native quantity qualification, frozen converted quotes, atomic multi-line replacement and historical reporting-currency snapshots are implemented. See [conversions](conversions.md). Overlapping approved amendments, live rates, indirect conversion and common-UOM analytics remain open.

## M7 — richer quoting and document delivery

Build typed quote sections, explicit optional lines, reusable versioned quote templates and deterministic package quantities. Keep individual pricing and cost evidence visible; do not introduce an opaque bundle total. Freeze template/configuration versions in quote submissions and exports.

Acceptance: choose a template, resolve required/optional products and allowed quantity relationships, reject an invalid configuration atomically, price the resolved lines, route review, export a readable quote document and reopen it unchanged. Test rounding, aggregate margins, missing products, stale template versions, mixed currencies and revision conflicts. Generated documents must identify synthetic demo identities and cannot imply an electronic signature or customer delivery.

## M8 — promotion and channel workflows

Model effective periods, eligibility, budgets and explicit precedence/stacking decisions for promotion plans. Channel claims require a source invoice or other declared evidence, duplicate detection and independent review. Every plan must explain its impact on invoice and pocket values.

Acceptance: plan approval, eligible/ineligible calculation, deterministic explanation, claim validation and audit. Historical projections must remain separate from recorded actuals. Do not claim uplift or causal effectiveness from synthetic scenarios.

## M9 — governed data and enterprise boundaries

Introduce an adapter boundary for authenticated immutable actor IDs, organization scope, delegated review rules and trusted role assignment. Replace caller-selected identities only after a concrete local or enterprise authentication design is reviewed. Add a storage adapter with explicit schema migrations, backups, restore drills and transaction-boundary tests before raising dataset limits.

Acceptance: actor/tenant isolation, denied impersonation, contributor exclusions across identity changes, migration rollback/recovery, retention behavior and load tests. Real identity providers, credentials and external integrations require separately authorized setup; prototype role labels never count as enterprise access control.

## M10 — integration delivery and operational analytics

Implement synthetic adapter fixtures before any live system. Imports stage validation and record source provenance. Outbound approved-price or quote delivery uses an outbox, stable external keys, acknowledgements and reconciliation. Analytics require explicit grain, currency partitioning, source revision and actual/projection distinction.

Acceptance: retries, partial failures, duplicate deliveries, out-of-order events, dead-letter inspection and exact reconciliation in a local simulator. A simulator does not establish SAP/BW/ERP API compatibility. Live read/write connections are separate deployment decisions.

## M11 — optimization research

Optimization starts with a stated decision, objective, constraints and data provenance. Demand/elasticity estimates need chronological validation and uncertainty measures. Compare against deterministic baselines and preserve guardrails, explanation, overrides and review provenance.

Acceptance: reproducible data split and baseline, out-of-sample evaluation, leakage checks and documented failure modes. No AI or optimization label until a working model and defensible evidence exist. Synthetic demo data cannot validate commercial benefit.

## Shared release gate

For every milestone: update exact command/model contracts, add meaningful unit/property/invariant and process regressions, request independent financial/persistence/authorization review, run layer/attribution/fmt/strict-Clippy/test gates, verify native builds and process/transport tests on Windows/macOS/Linux, inspect native UI screens and preserve Library handoff identity. Record source commit, CI run, downloadable artifact URL and checksum. Windows visual/accessibility validation and signed distribution remain explicit release work.
