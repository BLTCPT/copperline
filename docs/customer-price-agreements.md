# Customer price agreements

M5 adds immutable, independently approved customer price-contract editions. They wrap fixed unit prices or frozen approved grid/list origins with explicit date periods and quantity bands. Existing grid/list selections keep their original point-date behavior. All actions remain local and synthetic; no contract is delivered to a customer or external system.

## Terms and lifecycle

A contract family has a stable ID, owner, customer and currency. Each creation adds the next contiguous business `edition`, including after rejected or withdrawn drafts. Each edition freezes its name, start/end, integer priority, 1–100 stable-ID price rows, owner, source contributors and origin evidence. A row identifies a product/UOM, inclusive minimum quantity, optional exclusive maximum and a price basis. Date intervals are always `[start, end)` with an explicit end. Quantity bands are `[min, max)`; a null maximum is unbounded within the normal quantity limit. Rows for the same product cannot overlap.

The separate workflow `version` starts at 1. Owner Analyst/Admin may submit Draft (version 2) or withdraw Draft/Pending. Only an independent Approver may approve/reject Pending (version 3). Admin and Finance are not substitutes. Review excludes the contract owner and authors of captured grid/list origins. A grid author's identity comes from its immutable calculation audit event, not the mutable owner of a later candidate. Rejected/withdrawn records remain stored. There is no edit, delete or post-approval withdrawal command.

Creation with an existing family requires the same owner/customer/currency. Approval requires an edition greater than every earlier approved edition and a start date no earlier than those editions. Equal start dates permit same-period corrections. A lower pending edition that is overtaken cannot later activate; reject or withdraw it. Future starts remain inactive until the requested pricing date reaches them. The deterministic engine uses the request date and recorded approval revisions, not a wall clock or scheduler.

Contract approval governs commercial terms. It does not guarantee future profitability. Every quote still uses current effective costs and the quote's guardrails/review route; no contract bypasses a hard quote submission block.

## Source basis

- `fixed`: exact nonnegative invoice-unit money in the product's currency and UOM.
- `approved_source`: an explicit Grid or published PriceList `PriceRequest` with zero additional discount/rebate. Creation captures its approved invoice unit, source identity, approval/publication/capture revisions and source author. Contract minimum quantity cannot undercut the origin's approved minimum. Currency/UOM and customer/product must match. Contract-origin chaining is rejected.

A new independently reviewed contract explicitly authorizes its own date period. Replacing the original grid later does not change or invalidate the captured contract price. Creating a new contract from an obsolete grid selection fails; choose its current approval deliberately. Contract terms and exports do not consult current catalog names or recompute old origin prices.

## Selection and precedence

No `source` still means live catalog/rules. Contract selection is explicit:

| Source selector | Behavior |
|---|---|
| `{"kind":"contract","id":"CP-ATLAS","edition":1}` | Choose this exact active edition of this family. It must qualify by date, customer, product, quantity, currency and UOM. It may deliberately select a lower-priority family. |
| `{"kind":"customer_contract"}` | Search eligible approved customer contracts and select the highest integer priority. No eligible contract is an error; no fallback to live rules. |

For each family, choose the highest approved edition whose start is at/before the request date **before** testing expiry, product or quantity. An expired new edition, removed product or band gap never revives an older edition. Another eligible family may still apply in automatic mode.

Approval rejects equal-priority overlaps for the same customer/currency/product where both date and quantity intervals intersect. Adjacent half-open intervals do not conflict. Different priority is an explicit business precedence decision. Conflict checks use each edition's effective end, clipped at the next approved edition's start; superseded raw terms do not permanently block a new family. Same-start supersession leaves an empty effective window for the older edition.

Once selected, the contract invoice unit is the new negotiated-discount base. Source discounts are already included; source rebates are not inherited. The current request supplies its own discount and rebate. Current effective costs determine margin. There is no FX or UOM conversion and no assumed unit equivalence.

## Frozen and reproducible evidence

The original request retains automatic or exact selection intent. The evaluation pins a concrete family/edition, stable row ID, terms fingerprint, approval/capture revisions, price unit, effective interval, quantity band and priority. Snapshot validation reruns source resolution using only approvals known at that capture revision. Later same-start editions, priority changes, source replacements or gaps cannot rewrite historical quotes or approval cycles.

New optional fields omit serialization when empty, preserving old M1–M4 snapshot/receipt bytes. Existing non-contract quote CSV headers remain unchanged. Contract-backed quote CSV adds selection mode, row, interval, maximum quantity, priority and terms fingerprint. Approved contract CSV exports the immutable contractual interval, not a claim that this edition is still currently selected; later editions remain separate. Both exports refuse overwrites through UI/CLI.

Creation and review audit events bind fingerprints of immutable terms, rows, contributors and review data. Exact command, target, chronology and roles are checked, including orphaned audit targets. These are consistency checks over caller-selected demo identities, not authentication or adversarial tamper protection.

## Atomic quote-line replacement

`quote_replace_line` accepts `{id, version, index, request}`; index is zero-based. Only the quote owner in Sales/Admin role may replace a Draft line. The replacement must match the quote's customer/date. It first builds all requested lines, substitutes the selected request, recalculates every retained line against one current source revision/policy, checks aggregate arithmetic/currency, validates, persists and swaps state once.

An invalid replacement, stale retained source, stale version, wrong owner, overflow or disk failure leaves the entire quote and durable state unchanged. Replacing a stale selected source with a deliberate current/automatic/live selection can succeed; a different retained stale source still blocks the transaction. Quote approval history and contributor exclusions survive replacement. Exact retry commits once, including after reopen.

## API shape

Use `contract_create`, `contract_submit`, `contract_decide`, `contract_withdraw`; read full records through `state`, calculate through `evaluate`/`compare`, and retrieve CSV through `export_contract {id, edition}`. CLI: `export-contract ID EDITION NEW_FILE`.

A `contract_create` terms payload:

```json
{
  "terms": {
    "id": "CP-ATLAS", "edition": 1, "name": "Synthetic service prices",
    "customer_id": "C-ATLAS", "currency": "USD",
    "start": "2026-10-01", "end": "2027-01-01", "priority": 100,
    "rows": [
      {"id":"RETAIL","product_id":"VLV-100","unit":"each","min_quantity":1000,"max_quantity":10000,
       "basis":{"kind":"fixed","invoice_unit":9500}},
      {"id":"BULK","product_id":"VLV-100","unit":"each","min_quantity":10000,"max_quantity":null,
       "basis":{"kind":"fixed","invoice_unit":9000}}
    ]
  }
}
```

JSON money is cents, quantity thousandths and discount/rebate basis points. State keys are `contract_key(id, edition)` (for example `CP-ATLAS:1`). Commands also require the usual workspace revision, request ID and demo actor. Lifecycle commands pass separate `edition` and `version`; statuses are `Draft`, `Pending`, `Approved`, `Rejected`, `Withdrawn`.

## Boundaries

At most 500 editions and 5,000 total rows, still bounded by the whole-workspace snapshot size. No contract line editing after creation, multi-customer/segment contracts, formula stacking, unit conversion, currency conversion, tax/freight treatment, signed contract document, customer delivery or enterprise authorization. Larger-scale matching/storage and effective-dated conversion schedules remain future work.
