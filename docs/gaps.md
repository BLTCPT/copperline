# Gaps

- Full functional parity is a long-term target. Current milestone is synthetic local pricing/quote workflow; it is not a Pricefx replacement or compatible API.
- No enterprise identity, multi-tenancy, concurrency across users, delegated approval hierarchy, quote ownership enforcement, encryption or signed artifacts.
- No predictive optimization, elasticity/demand estimation, AI recommendations, enterprise analytics warehouse, competitor data, CPQ configuration/BOM, tax/freight engine, contract settlement or ERP adapters.
- Native UI focuses on USD/each; command API supports EUR/GBP and named units. No FX or unit conversion.
- Snapshot history uses full state per revision and scans on open. Bound limits suit demos, not enterprise volume. Recovery/export/migration ergonomics need expansion.
- Live grid has candidate/last-approved lifecycle and lists have whole-document approval. Neither automatically writes into quote lookup or external systems; quote evaluation uses explicit current rules.
- Rebate agreements support approved retrospective tiers and frozen accruals. No amendments, renewals, progressive tiers, credit memos, settlement, reconciliation or payment posting.
- Rules are single-winner. No arbitrary scripting, stacked promotions, formula editor or conditional approval routing.
- Windows release compilation and CLI runtime passed baseline CI. Interactive Windows desktop behavior remains unverified. UI accessibility and small-screen coverage need deeper testing.
