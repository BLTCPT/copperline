# Gaps

- Full functional parity is a long-term target. Current milestone is synthetic local pricing/quote workflow; it is not a Pricefx replacement or compatible API.
- No enterprise identity, multi-tenancy, concurrency across users, delegated approval hierarchy, quote ownership enforcement, encryption or signed artifacts.
- No predictive optimization, elasticity/demand estimation, AI recommendations, analytics actuals warehouse, competitor data, CPQ configuration/BOM, tax/freight engine, contract settlement or ERP adapters.
- Native UI focuses on USD/each; command API supports EUR/GBP and named units. No FX or unit conversion.
- Snapshot history uses full state per revision and scans on open. Bound limits suit demos, not enterprise volume. Recovery/export/migration ergonomics need expansion.
- No persistent per-item live-grid candidate/last-approved lifecycle yet; workbench is live evaluation only. Lists have whole-document approval and local publication.
- Rebate is quote-level line input with exact accrual calculations; no agreement tier settlement/reconciliation yet.
- Rules are single-winner. No arbitrary scripting, stacked promotions, formula editor or conditional approval routing.
- Native Windows runtime requires CI/desktop evidence; macOS compilation does not verify it. UI accessibility and small-screen coverage need deeper testing.
