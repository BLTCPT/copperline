# Explicit approved pricing sources

Every evaluate, scenario and quote-line request chooses its pricing basis. Omitting `source` retains the live catalog/rule precedence documented in architecture.md. Selecting a source never falls back to live rules if that source is invalid, unavailable or stale.

| Selection | Eligibility and precedence |
|---|---|
| No `source` | Current effective catalog, cost and winning pricing rule |
| `{"kind":"contract","id":"CP-ATLAS","edition":1}` | Exact active customer contract edition with approved date interval and quantity band |
| `{"kind":"customer_contract"}` | Opt-in highest-priority eligible customer contract; missing or ambiguous coverage fails |
| `{"kind":"grid","id":"G1","approval_revision":4}` | Exactly this item's current independently approved price; a newer Draft/Pending/Rejected candidate cannot override it |
| `{"kind":"price_list","id":"L1","version":4}` | Exactly this published list version, with exactly one matching product line; an approved but unpublished list is ineligible |

The numeric identifiers above are examples. Read the workspace and select the actual active grid approval revision or published list version. Direct grid/list selection does not search for a best approved price. Lists and grids therefore cannot silently compete with each other or with rules. Only the explicit customer-contract selector searches eligible contracts, using their approved priorities.

**Direct grid/list validity remains narrow:** those approved records are point-date pricing snapshots. The request date must equal the source pricing date, the product/customer/currency/UOM must match, and request quantity must be at least the source's calculated quantity. That calculated quantity is the minimum qualification for the chosen price. There is no inferred expiration period, open-ended contract, FX conversion or unit conversion. Create and approve a new source for a new date. For cross-date validity and quantity bands, create an independently approved [customer price agreement](customer-price-agreements.md) around a fixed price or frozen grid/list origin.

The source's approved **invoice unit** becomes the base for the request's negotiated discount. For example: catalog 100.00 → live rule 90.00 → grid's own negotiated discount 81.00; selecting that grid with a new 10% discount yields 72.90. It does not rerun the old rule or apply the old discount again. The source's rebate is not inherited: only the new request's rebate applies to the newly extended invoice, rounded once. Current effective cost still determines the quote's cost and margin. Guardrails continue to use catalog-relative effective discount and the quote's own pocket margin.

Grid candidates themselves must calculate from live rules. Chaining a selected source through another grid/list source is unsupported and rejected, avoiding recursive discount/rebate interpretation.

## JSON request example

```json
{
  "product_id": "VLV-100",
  "customer_id": "C-ATLAS",
  "date": "2026-10-13",
  "quantity": 1000,
  "discount": 1000,
  "rebate": 0,
  "source": {"kind": "grid", "id": "G1", "approval_revision": 4}
}
```

Amounts retain API conventions: quantity is thousandths of the product unit and rates are basis points. This object is used as the `request` payload in `quote_add_line`, or as an evaluate/scenario request. Older requests without the field serialize identically; the absent field is not written into snapshots or idempotency fingerprints.

## Frozen evidence and safe repricing

Evaluation `source` evidence pins the selected document, document version where available, pricing revision, approval revision, publication revision for lists, capture revision, product/customer/currency/UOM, exact date, minimum quantity and approved invoice unit. Legacy M2 grid prices have no document-version field; their immutable approval revision still identifies the selected version. Newly approved grid prices record both identities.

On grid replacement, the old approved record is retained in `approved_history` keyed by approval revision. Existing quotes and submission snapshots continue validating against that immutable record and retain their original price. A new evaluation or a Draft quote reprice using the superseded grid revision fails explicitly; use atomic `quote_replace_line` with a deliberate current selection. No stale source is silently upgraded. Source evidence cannot come from a revision later than the quote's calculation or before the source approval/publication.

Approved quote CSV exports include the selected source label, document version, pricing/approval/publication/capture revisions, approved source invoice unit and minimum quantity. Historical exports remain stable after grid updates. Selecting a source, freezing an approval and publishing a list remain local engine operations; no external commercial system is written.

## Regression coverage

`crates/engine/tests/pricing_sources.rs` covers explicit selection, source/candidate separation, discount and rebate rounding, exact date and quantity eligibility, product/customer/UOM rejection, unpublished source rejection, list item ambiguity, frozen evidence, stale-source transactional failure, grid replacement history, legacy serialization and durable reopen. Tests operate through the engine for complete lifecycle transitions and mutate cloned snapshots only to verify corruption rejection.

M5 contract sources retain automatic/exact request intent and pin concrete edition, row, interval, quantity band, priority and terms fingerprint. Resolution is reconstructed at the captured revision. Existing point-date sources and old CSV bytes remain compatible. See [contract semantics](customer-price-agreements.md).
