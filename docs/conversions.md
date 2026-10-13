# Explicit dated FX and units

M6 adds a bounded conversion workflow shared by the native desktop, command engine and local API. All examples are synthetic; no live exchange-rate feed, inferred unit equivalence or enterprise approval identity is present.

## Direction and approval

A conversion book has an ID, name and 1–100 stable-ID rows. Every row has a direct key, positive rational `numerator/denominator` (both integers 1 through 1,000,000,000), inclusive start and exclusive end. `fx` means **target currency per source currency**. `uom` means **native product units per requested unit**, scoped to exactly one product. UOM destination must equal that product's immutable native unit. The supported monetary currencies are USD, EUR and GBP, all with two decimal places.

For example, USD→EUR 9/10 means USD 100.00 becomes EUR 90.00. VLV-100 box→each 12/1 means 1 box contains 12 native each. A request for EUR/box needs both legs. Identity legs omit rates. A conversion request whose currency and unit are both already native rejects as redundant.

Terms are immutable from creation. Analyst/Admin owner creates/imports Draft v1, submits Pending v2, and a distinct named Approver approves or rejects v3. Admin cannot override reviewer role or self-review exclusion. The owner can withdraw Draft or Pending books. Rows may not overlap the same direct key and date within a book or across approved books. Approval rechecks this after concurrent proposals. Adjacent periods use new rows/books. There is no overlapping approved amendment, deletion or automatic rate replacement.

No inverse, triangulation or graph search is inferred. An independently declared reverse FX row need not be mathematically reciprocal. Missing/incompatible/directly ambiguous schedules fail. Money rounding means even reciprocal declared rates do not promise a round trip. Native values are preserved; historical evidence is never reconstructed using an inverse.

## Quote order of operations

1. Validate the explicit requested currency/unit and pricing date. Find the directly approved FX/UOM rows known at the calculation revision.
2. Convert requested quantity to native thousandths exactly. Reject a remainder, nonpositive result or bounds violation. Both requested and normalized quantities must fit the quantity limit.
3. Qualify rules, approved grid/list sources and contract quantity bands using this **native quantity and native unit**. Existing source-date rules still apply. The approved source remains a native unit-price basis; its original discount is included and its rebate is not inherited.
4. For each native list, selected-rule/source and cost unit amount, compute native cents × UOM numerator × FX numerator / (UOM denominator × FX denominator) using checked integer arithmetic. Round once to target cents, half away from zero. There is no intermediate FX/UOM rounding.
5. Apply existing negotiated-discount unit rounding, extend by requested quantity, then round the extended rebate. Margin equals pocket less extended converted cost. Approval guardrails and value thresholds use the resulting target currency.

This **unit-first** convention does not guarantee equal totals across different units, quantity splits or round trips. A very small converted unit price can round to zero; zero pocket makes margin percentage undefined. The quote shows the selected unit and full native basis. The rounding policy is frozen as `native_unit_combined_half_away_then_extend_v1`.

Grid/list/contract authoring remains native. Attempting to use a converted request to author a reusable source rejects; a converted quote is not another source. Each reprice starts from original product/source amounts, never a previously converted value. Scenario comparisons require the same output currency. Quote totals never combine currencies implicitly.

Every converted evaluation retains native currency/unit/quantity and native list/rule/cost values plus exact rate rows, direct direction, effective interval, immutable book fingerprint, workflow version, approval revision and capture revision. Quote submission freezes the full evaluation in its approval cycle. Historical validation uses the original capture revision, independently of newer approved schedules.

## Atomic multi-line replacement

`quote_replace_lines` takes an existing Draft quote ID/version and distinct zero-based replacement indices. It replaces all requested positions in a copied request set and reevaluates every retained line, then checks totals and common currency before one commit. A failed conversion/source/version/aggregate check leaves all memory and committed bytes unchanged. This supports changing every quote line's currency and repairing multiple stale retained sources together. Prior approval cycles and contributor exclusions remain intact.

In **Quotes**, expand the replacement form, select a line and enter its new request, then **Stage selected line**. Repeat for other lines and **Apply staged replacements**. Staging captures the quote version; a later edit requires restaging. Nothing is saved by staging alone.

## Reporting currency

`converted_actuals` explicitly chooses a target currency and half-open date period. It reads recorded invoice actuals known at a source revision. Each invoice total and cost total converts **separately once at the original invoice date**, then aggregates per customer in the selected reporting currency. Margin is converted invoice minus converted cost, not separately converted margin. The frozen convention is `each_recorded_total_half_away_then_aggregate_v1`.

An approved full reversal subtracts the exact original converted invoice/cost pair using the original date/rate, regardless of correction record date. A replacement qualifies independently with its own native context/date. Historical reports exclude later invoice ingestion, correction approvals and rate approvals. Any missing rate rejects the whole result.

`report_convert` saves an immutable report ID with source revision, original invoices, reversal IDs, rates and groups. Later corrections or schedules do not rewrite it. Native product quantities/units stay visible; reporting does not add incompatible quantities or normalize them to a common UOM. Reports exclude quotes, rebate settlements, tax and freight and never post or pay money. A report is an audited calculation snapshot, with no separate approval workflow.

## CSV and limits

Import columns are `id,kind,product_id,from,to,start,end,numerator,denominator`. FX rows leave product_id empty; UOM rows require it. See `fixtures/conversions.csv`. CSV import creates one immutable Draft book atomically and is limited to 1 MiB and 100 rows. Whole-workspace limits are 100 books/5,000 rate rows, 100 reports/5,000 total frozen invoice rows, 5,000 invoices per report, and the existing 16 MiB state limit.

Approved-book CSV includes terms, direction, dates, ratio, reviewer, version and fingerprint. Converted-quote CSV conditionally appends native context, rounding policy and full FX/UOM evidence JSON. Old quotes without conversion retain their original CSV bytes. Frozen-report CSV includes native and converted amounts, original date/UOM, reversal ID, net contribution, rounding and full FX evidence. CSV cells are formula-safe; CLI/UI exports create new files only. Empty conversion maps and absent optional fields preserve old snapshot/receipt serialization.

Deferred: live rate acquisition, rate amendment/revocation, bid/ask/feed conventions, arbitrary currency precision, inferred or multihop conversion, dimension inference, analytics UOM aggregation, tax/accounting FX treatment, enterprise authentication, and external postings. These are explicit boundaries, not silently assumed behavior.
