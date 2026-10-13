# Roadmap after M9

Reviewed 2026-10-13. This is a proposed order for further work, not a delivery commitment or a claim of full Pricefx parity. The [capability map](parity.md) provides current public-source comparisons and executable evidence; [gaps](gaps.md) records operating limits. Keep each increment runnable through the engine, CLI/API and independent native UI.

## Delivered baseline

M1–M3 established deterministic pricing, reviewed lists/grids, explicit approved sources, frozen quote approval routes and local recovery. M4 added immutable invoice correction and local rebate reconciliation. M5 added customer price-contract editions; M6 added explicit dated FX/UOM and reporting; M7 added exact-history archives and measured local workloads. M8 added full approved-quote booking. M9 added cumulative quantity credits and nominal invoice-period close. Quantity credits and nominal close are therefore delivered subsets, while partial booking, value-only credits and accounting close/reopen remain gaps.

The baseline is runtime source `a36d4877228b14732aff84d77d32776e9c8260ae`, evidence commit `64dd073`, and [three-OS CI run 38390734565](https://github.com/BLTCPT/copperline/actions/runs/38390734565). [M9 verification](../evidence/m9-verification.md) records 255 Rust checks, eight process flows, historical compatibility and artifact identities. Historical M7 timing results remain [separately scoped](performance.md).

## Recommended next local build: usable commercial documents

Make the existing approved pricing workflow useful for a human handoff before adding another financial subsystem. Add original, versioned quote templates and a readable local document export with customer context, validity, quantities, currency/unit and approved price totals. Provide an internal evidence appendix showing the exact source/approval/conversion versions; keep internal cost/margin information explicitly separate from the intended customer-facing content. Add a compact review queue and links from a booking or credit to its frozen source evidence.

This builds directly on the existing quote/booking models and needs no company data or external connection. Initial scope should remain flat quoted products; bundles, optional products and BOM configuration are later work, each with explicit quantity and pricing semantics. Local generation does not send a document, issue a legal invoice or apply an electronic signature.

Acceptance: generate from an approved quote, preserve totals and frozen version after later catalog changes, reject an ineligible draft, escape text safely, retain provenance through reopen, and inspect representative Windows/macOS exports and native screens. Test that internal-only fields cannot enter the customer-facing template by default. Reuse [quote process](../scripts/e2e.py), [booking process](../scripts/bookings_e2e.py) and [native tests](../crates/ui/src/bookings/tests.rs) as regression baselines.

## Next: repeatable data handoff and reconciliation

Build a local import-staging and outbound-delivery simulator around explicit, versioned schemas. Show mapping/validation errors before committing a batch; freeze file/source keys and provenance. An outbox should carry an approved price or quote version with a stable delivery ID, acknowledgement and reconciliation status. This is the shortest path from a self-contained demo toward an ERP/BW pricing workflow without pretending a connector already exists.

Acceptance: deterministic synthetic round trip, atomic invalid import, duplicate and out-of-order input, delivery retry, partial failure, stale approval, acknowledgement mismatch and exact reconciliation. No real SAP/BW compatibility or external posting is implied. A real adapter follows only after the target interface, authoritative identifiers and access/write scope are known. Preserve the current [API boundary](api.md) and [transport regressions](../scripts/transport_e2e.py).

## Required before a shared pilot: identity and measured storage

Introduce trusted immutable actor IDs, authenticated role assignment and organization scope before sharing real data or allowing remote clients. Preserve historical contributor exclusions across renamed/disabled users. Define deployment and threat boundaries, then test denied impersonation and cross-organization access. Delegation must have explicit authority, dates and audit evidence; local role labels cannot substitute for authentication.

Measure the intended dataset and history before selecting a transactional storage adapter. Require schema migrations, full restore drills, crash/failure tests and retained history/receipt equivalence. Benchmark M9 workflows as well as imports and quotes with stated hardware, cache conditions, sample counts and concurrent users. Do not increase caps or claim capacity from the existing [small/medium workload](performance.md). This work is a gate for a shared pilot, even if it starts before the preceding local enhancements finish.

## Then: agreement lifecycle and operational analytics

Extend the existing contract-edition mechanism one coherent policy at a time: formula/index clauses, explicit amendment comparisons, renewal review, and invoice-versus-agreement exceptions are useful candidates. Build source-linked reports that distinguish invoice date, approval revision, recorded versus reporting currency, and actual versus projection. Any later partial booking needs an explicit remaining-quantity model; any close reopening/carry-forward needs new, independently reviewed records rather than edits to frozen evidence.

Acceptance: old quotes/accruals/reports remain reproducible, new terms apply only to the intended intervals, no expired-edition resurrection, exact credit/settlement baselines and deterministic exception explanations. Retain [contract regressions](../crates/engine/tests/contracts_m5_review.rs), [credit invariants](../crates/engine/tests/m9_credits.rs) and [close regressions](../crates/engine/tests/m9_periods.rs). Progressive or growth rebates require an agreed calculation policy before implementation; do not infer it from the existing retrospective tiers.

## Later, as separate workflows: promotions and channel claims

Promotions need approved dates, eligibility, budgets and explicit precedence/stacking, then actual-versus-plan reporting. Channel claims need distributor/POS evidence, agreement matching, quantity capacity, duplicate detection, disputes and independent review. Build these as separate milestones because a campaign and a reimbursement claim have different evidence and financial lifecycles.

Acceptance: explain eligible/ineligible decisions, prevent duplicate consumption, preserve prior approvals, and reconcile exact invoice/pocket effects in synthetic fixtures. Settlement remains local until an external posting contract is authorized. Do not label synthetic scenario differences as causal promotional uplift. The public workflow expectations are linked in [the matrix](parity.md).

## Last: optimization and AI research

Start from a named business decision, objective, constraints and authorized data provenance. First establish deterministic benchmark policies and clear actual-outcome measures. Demand, elasticity or win-rate estimation needs chronological holdouts, leakage checks, uncertainty and failure analysis. Recommendations must retain explanation, guardrails, overrides and approval provenance.

Acceptance requires a working model evaluated against the benchmark on unseen data, with reproducible splits and documented limitations. Synthetic data can test mechanics but cannot demonstrate commercial benefit. Monitoring rules can ship independently as transparent alerts; they must not be called trained AI or optimization.

## Decisions for Mark versus implementation work

| Mark's decision, when relevant | Why it changes scope |
|---|---|
| First real workflow and users: price maintenance, sales quoting, or rebate/claims operations | Selects which later business slice and interface deserve priority. The next local document/handoff increment can proceed with synthetic defaults. |
| Local single-user tool or shared organizational pilot; intended data sensitivity | Determines the identity, deployment, storage and distribution requirements before real-data use. |
| Target ERP/BW/CRM interface, authoritative data owner and approved read/write scope | A real connector needs actual schemas and explicit access; no endpoint, identity or compatibility should be guessed. |
| Business policies for partial fulfillment, renewals, tier method, closed-period corrections, promotions or claims | These are commercial/accounting choices, not routine coding defaults. Only request the policy needed by the selected slice. |
| Optimization decision/objective and suitable historical data | Determines whether research can be evaluated meaningfully at all. |

Crate layout, neutral UI presentation, synthetic fixtures, error messages, deterministic algorithms, migration mechanics and test organization are implementation decisions within an agreed slice. Continue those without repeated permission requests. Real-data access, deployment, external delivery and business writes are separate authorization boundaries; a roadmap item does not authorize them.

## Shared release gate

Preserve engine-first layering and exact money/history invariants. Update command/model contracts and scope limits, add financial/authorization/persistence and process regressions, obtain independent review, and run `cargo xtask ci`. Verify release builds and process/transport workflows on Windows/macOS/Linux; inspect the native UI and new documents on their target platforms. Record source commit, CI run, exact artifact hashes and handoff identity. Treat Windows visual/accessibility testing, signed distribution and measured scale as explicit work until evidence closes each gap.

Morning usability/package stabilization is recorded separately in [current verification](../evidence/verification.md); the M9 financial capability scope above remains unchanged.
