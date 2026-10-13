# Functional capability map

Reviewed 2026-10-13. Target: broad functional parity with publicly documented Pricefx workflows over successive milestones. This is a research checklist, not an exhaustive feature inventory, compatibility guarantee or completed-parity percentage. Implementation is independent; only public documentation informs workflow expectations.

“Implemented” means concrete behavior exists; test evidence is listed separately. “Partial” identifies narrower semantics. “Missing” means no working equivalent. No empty module is counted as implemented.

| Capability | Copperline status | Evidence / boundary |
|---|---|---|
| Central product/customer repository, unit/currency | Implemented | Model validation, CSV regression; synthetic catalog |
| Effective cost and missing-cost handling | Implemented | Pricing golden/missing-cost tests |
| Versioned rules and explicit precedence | Implemented | Rule precedence/effectivity tests; single-winner fixed/discount/markup |
| Price lists, whole-list approval, distinct publication | Implemented local subset | Engine lifecycle and process E2E; no external delivery |
| Live grid calculations | Partial | Workbench evaluates current data; no item-level approved-price history yet |
| Candidate recalculation without replacing last approved grid item | Missing | Roadmap; must preserve approved values during recalculation |
| Quote creation, line edits, discount and rebate waterfall | Implemented | Golden financial case and E2E |
| Frozen quotes and separate-person approval | Implemented local subset | Lifecycle/immutability tests; caller-selected demo identities |
| Configurable multistep approvals, delegation, thresholds by role | Partial | One explicit review step, hard/soft guardrails, no routing engine |
| Scenario comparison | Implemented deterministic subset | Same product/customer comparison; no demand estimation |
| Analytics actuals, segmentation, dashboards, alerts | Partial | Weighted quote totals; no actuals warehouse or historical performance |
| Customer agreements / contract terms / renewals | Missing | No agreement lifecycle yet |
| Rebates, tier programs, accrual/settlement/reconciliation | Partial | Exact per-line percentage accrual only; no settlement |
| Promotions and promotional effectiveness | Missing | No calendar, uplift/demand or budget model |
| Channel management / distributor claims | Missing | No channel program or claims workflow |
| CPQ configuration, bundles, templates, quote document delivery | Partial | Flat quote items and CSV export; no configurable BOM or document templates |
| Optimization and AI deal/price recommendations | Missing | No model, optimization engine or unsupported claims |
| Integrations / ERP / CRM / external market data | Partial | Documented command boundary and CSV; no live adapters or write-back |
| Enterprise identity, tenant isolation, scalable data pipelines | Missing | Explicit security boundary; local single-writer snapshots |
| Audit, revision conflict detection, retry safety | Implemented local subset | Durable snapshots, receipts, tests; no external immutable audit service |
| Native Windows desktop | In verification | Rust/egui implementation and Windows CI; evidence tracked separately |

## Public workflow references

- [Price management](https://www.pricefx.com/platform/price-management/): pricing rules, simulation, approval and publishing inform the central workflow.
- [Quoting](https://www.pricefx.com/platform/quoting/): quote creation, transparent price components and review guide the quote milestone.
- [Price lists](https://knowledge.pricefx.com/configuration-engineer-knowledge-base/guides/price-setting-concepts/price-lists): point-in-time list calculations and document-level approvals.
- [Live price grids](https://knowledge.pricefx.com/pricefx-unity-documentation/pricefx-capabilities/price-setting/live-price-grids): item-level active pricing is distinct from frozen list documents.
- [Public platform navigation](https://www.pricefx.com/platform/price-management/): agreements, promotions, rebates, channel management and optimization define additional roadmap areas.

No vendor screenshots, examples, assets or implementation were imported. Advanced capabilities remain gaps until implemented and verified against explicit acceptance criteria.
