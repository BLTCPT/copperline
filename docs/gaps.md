# Gaps and operating boundaries

As of 2026-10-13, tested M9 source is `a36d4877228b14732aff84d77d32776e9c8260ae`; [verification](../evidence/verification.md) records the artifacts and checks. Copperline is a synthetic local workbench with working pricing, review and invoice-evidence workflows. It is not a Pricefx replacement or compatible API. The [capability map](parity.md) distinguishes tested subsets from partial and missing features using official public sources; the [roadmap](roadmap.md) prioritizes future work.

## Boundaries of the working workflows

| Area | Implemented boundary | What remains absent or restricted |
|---|---|---|
| Prices and contracts | Effective costs, deterministic single-winner rules, reviewed grids/lists and approved customer contract editions | No formula/index engine, arbitrary scripts, stacked promotions, multi-customer contract terms, bulk contract import or renewal workflow. Explicit grid/list sources retain exact-date and quantity-floor semantics; contracts provide date periods/bands. Source authoring stays catalog-native. |
| Quoting and review | Flat priced lines, atomic draft replacement, frozen commercial → conditional Finance routes | No bundles/BOM, optional/configurable sections, rendered quote templates, customer delivery, automatic approval, parallel review, delegation or general organization hierarchy. |
| FX and UOM | Approved dated direct rational rates, exact native thousandths, frozen converted quotes and reporting | USD/EUR/GBP with two money decimals only. No inferred inverse/multihop, live feeds, overlapping approved rate amendments or common-UOM analytics. Converted units round before quantity extension; this can differ from converting an extended native total. The UI catalog editor uses USD. [Conversion semantics](conversions.md). |
| Booking and reporting cutoff | One entire approved quote booking per quote ID lifetime; exact recorded totals and separate non-posted native basis | No partial booking, fulfillment/shipment tracking or customer invoice issuance. Booked reporting in another currency requires a direct rate known before booking for the invoice date; later rate approvals cannot repair missing eligibility. Native preflight warns for USD/EUR/GBP. Raw invoice add/import remains catalog-native. [Booking semantics](bookings.md). |
| Corrections and credits | Full reversals or cumulative quantity credits; raw invoices also allow atomic reversal/replacement | Credits and full corrections cannot mix on an original. No arbitrary value-only credits, credit amendments/reversals, tax/freight adjustment or legal credit-note delivery. Replacing a booked commercial transaction requires a distinct approved quote and new booking. [Credit semantics](credits-periods.md). |
| Rebates and settlement | Approved retrospective revenue tiers, frozen full-period accrual alternatives and signed additive local settlement adjustments | No progressive/growth/baseline programs, forecasting, agreement amendments/renewals, external reconciliation or payment/ERP posting. A signed amount can be negative; it is not a digital signature or a cash payment. |
| Period close | Finance-reviewed nominal invoice-date interval in one recorded currency freezes exact invoice/correction/credit basis | No accounting-period compliance, reopen, next-period carry-forward or inventory valuation. Reporting FX and rebate calculations are outside the nominal close. Closed originals cannot receive later-dated credits as a workaround. [Close semantics](credits-periods.md). |
| Analytics | Customer/currency invoice-margin summaries and frozen reporting-currency snapshots include reversals and credits | No warehouse, configurable multidimensional dashboards, advanced segmentation, automated alerts or competitor feeds. Partial credits do not decrement the active original count; full quantity exhaustion does. Recorded actuals, projected quote pocket values and alternative rebate accruals must not be summed together. |

## Missing commercial capabilities

Promotions require their own calendar, eligibility, budget, offer-stacking and outcome model. Distributor/channel claims require partner/POS evidence, agreement matching, duplicate detection and exception review. Neither is supplied by the existing discount, invoice-credit or rebate records.

There is no optimization engine, demand/elasticity estimation, win-rate model, AI recommendation, agent or copilot. Deterministic scenarios cannot establish uplift or commercial benefit. Research would require an explicit decision objective and suitable authorized historical data before model validation is meaningful.

## Before a shared or real-data pilot

Actor names and roles are caller-selected. Authentication, trusted immutable identities, SSO, tenant isolation, delegated authority and impersonation resistance are missing. There is no encryption-at-rest, signed release/update channel or external immutable audit service. Host-account file protection and local consistency hashes do not supply these controls. See [security boundaries](security.md).

CSV and local commands are working integration surfaces, but there is no live ERP, SAP, BW, CRM or market-data adapter, managed synchronization, outbound acknowledgement or accounting reconciliation. No company schema, data, credentials or connection has been validated. A synthetic adapter could test retries and mappings; it would still not establish compatibility with a real system. Real-data access and any external business writes require a separate authorized scope.

## Storage, scale and platform verification

Every revision stores full state; opening validates the complete history. The 16 MiB snapshot cap, 2 MiB request cap and 1 MiB CSV cap are safety limits, not supported enterprise capacities. Sixteen loopback API clients share serialized command execution. There is no database migration, retention-management policy, incremental backup or resumable restore. Complete-history backup and new-directory restore are implemented; see [backup limits](backup.md).

[Historical M6/M7 measurements](performance.md) cover one macOS host, warm caches, up to 1,000 ordinary invoices, 20 quotes and 327 snapshots (about 89.3 MiB history). They do not benchmark the new M8/M9 booking/credit/close workflows, cold starts, sustained concurrent load or enterprise volumes. The runner's 512 MiB history ceiling is a workload guard, not a product storage promise. M9's small workload/archive drill is regression evidence, not a fresh performance study.

M9 release builds, eight process flows, the small workload/archive drill and Windows packaging passed [Windows/macOS/Linux CI](https://github.com/BLTCPT/copperline/actions/runs/38390734565). Interactive Windows desktop behavior, deeper accessibility/small-screen coverage and abrupt power-loss durability remain unverified. macOS screenshots and Linux/Windows CLI success do not close those gaps.
