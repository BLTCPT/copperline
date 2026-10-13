# Gaps

- Full functional parity is a long-term target. Current milestone is synthetic local pricing/quote workflow; it is not a Pricefx replacement or compatible API.
- No enterprise identity, multi-tenancy, authenticated multi-user permissions, delegated approval hierarchy, encryption or signed artifacts.
- No predictive optimization, elasticity/demand estimation, AI recommendations, enterprise analytics warehouse, competitor data, CPQ configuration/BOM, tax/freight engine, external settlement/payment reconciliation or ERP adapters.
- Native UI focuses on USD/each; command API supports EUR/GBP and named units. No FX or unit conversion.
- Snapshot history uses full state per revision and scans on open. Bound limits suit demos, not enterprise volume. Inspect/quarantine recovery exists; backups, migration and retention ergonomics need expansion.
- Live grid has candidate/last-approved lifecycle and lists have whole-document approval. Quotes may explicitly select active grid or published list prices, with exact-date/minimum-quantity qualification. Open-ended contract pricing and validity intervals remain gaps; nothing writes external systems.
- Rebate agreements support approved retrospective tiers and frozen accruals. Full reversals/atomic replacements and signed local settlement recognition exist. Partial credit allocation, amendments, renewals, progressive tiers, customer credit-note delivery, accounting-period close, external settlement reconciliation and payment posting remain missing.
- Rules are single-winner. No arbitrary scripting, stacked promotions, formula editor or delegated/general approval routing. Quotes support frozen commercial → conditional Finance review.
- Windows release compilation, CLI correction/reconciliation workflows and concurrent API/recovery tests passed M4 CI. Interactive Windows desktop behavior remains unverified. UI accessibility and small-screen coverage need deeper testing.
