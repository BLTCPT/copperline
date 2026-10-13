# Command and integration contract v1

Run `copperline-cli --data data/api api` for newline-delimited UTF-8 JSON on stdin/stdout. `serve 7978` uses the same protocol on 127.0.0.1, explicitly opt-in and trusted-local only. Responses are `{ "ok": true, "result": ... }` or `{ "ok": false, "error": "...", "revision": n }`. Error lines do not terminate a healthy stdio session. Maximum input is 2 MiB per line; over-limit input closes the connection.

Mutation envelope:

```json
{"method":"execute","params":{"request_id":"seed-1","expected_revision":0,"actor":{"name":"Demo admin","role":"admin"},"command":"seed_demo"}}
```

Read methods: `state`, `catalog`, `evaluate`, `compare`, `export_quote`, `analytics`. `evaluate` accepts a PriceRequest:

```json
{"method":"evaluate","params":{"product_id":"VLV-100","customer_id":"C-ATLAS","date":"2026-10-13","quantity":10000,"discount":1000,"rebate":500}}
```

Wire units are integer cents, integer thousandths of the declared product unit and integer basis points. Dates are ISO calendar dates, supported years 2000–2200. Supported currencies: USD, EUR, GBP. `compare` takes `baseline` and `scenario` PriceRequests. `export_quote` takes `{ "id": "Q-1" }` and returns CSV text only for an approved quote.

## Commands

The exact Rust schemas live in `crates/engine/src/lib.rs` (`Command`, `Request`) and `crates/model/src/lib.rs`. All variants are snake_case; payload is in `params`.

| Command | Payload |
|---|---|
| seed_demo | no params; empty workspace, Admin |
| product_put / customer_put | complete Product / Customer |
| cost_add / rule_add | complete next-version Cost / Rule |
| policy_set | min_margin, max_discount, hard_min_margin (bps), Admin |
| products_import | csv string; creates only, atomic |
| quote_create | id, title, customer_id, date |
| quote_add_line | id, version, request (PriceRequest); refreshes draft lines against one source revision |
| quote_remove_line | id, version, index (zero-based) |
| quote_reprice / quote_submit / quote_revise | id, version |
| quote_decide | id, version, approve boolean, note |
| list_calculate | id, name, customer_id, date, quantity, currency |
| list_submit / list_publish | id, version |
| list_decide | id, version, approve boolean, note |

Mutation response contains committed revision, replayed flag and detail. expected_revision protects the whole workspace; document version protects transitions. Retry with exactly the same request ID/content to recover an uncertain result. Using that ID for changed content is an error. Failed commands have no receipt and may be corrected/retried. The engine does not silently overwrite concurrent changes.

## Future enterprise adapters

A future adapter must map external stable IDs into Product/Customer/Cost commands, preserve source dataset/version and ingestion audit, explicitly map currency/UOM, and stage a validated import before commit. An authenticated gateway must inject verified actor IDs/roles; it must never trust the client-supplied demo actor. Approved export should have an outbox acknowledgement/deduplication protocol, reconciliation and retry-safe external keys before any ERP write capability is enabled. There is no SAP/BW/Pricefx API compatibility claim, SDK dependency, live adapter or business write in this build.

Import CSV schema: `id,name,family,unit,currency,list_price`. Price uses plain decimal currency (`100.00`), not cents. CSV exports include document version, source revision, selected rule, cost version, currency/unit and frozen totals. Text is quoted and formula-sensitive prefixes escaped.

## M2 commercial workflows

Additional commands:

| Command | Payload |
|---|---|
| grid_calculate | id, version (0 for a new item), request (PriceRequest) |
| grid_submit | id, version |
| grid_decide | id, version, approve, note |
| agreement_create | terms: id, name, customer_id, currency, start, end, tiers [{threshold cents, rate bps}] |
| agreement_submit | id, version |
| agreement_decide | id, version, approve, note |
| transaction_add | Transaction: id, source, product_id, customer_id, date, quantity, unit, currency, invoice_total, cost_total |
| transactions_import | csv string; duplicate external IDs reject the entire batch |
| rebate_accrue | id (new snapshot), agreement_id, version (approved agreement version) |

Read `analytics` returns recorded invoice aggregates per customer/currency. It excludes quote projections and all accrual snapshots. Agreement start is inclusive, end exclusive. Approved agreement terms never mutate. Accrual uses retrospective tiers on eligible invoice totals. Recalculate accruals under new IDs; do not sum old and new snapshots.

Transaction CSV columns: `id,source,product_id,customer_id,date,quantity,unit,currency,invoice_total,cost_total`. CSV money is decimal major units and quantity decimal product units, while JSON money and quantity are scaled integers. See fixtures/transactions.csv. All fixture/source labels clearly identify synthetic data.

## M3 governance, sources and transport

`quote_approval_policy_set` takes `{expected_version, thresholds: {max_discount, min_margin, usd_value, eur_value, gbp_value}}`; Admin only. Percentages are basis points and amounts cents. Quote authoring/submission is owner-only with no Admin bypass. `quote_decide` remains the command for each sequential review; use the returned current document/workspace revision. Required roles are `approver`, then conditional `finance`, with distinct non-contributors. See [quote governance](quote-governance.md) for exact triggers, migration and frozen policy semantics.

PriceRequest adds optional `source`: `{"kind":"grid","id":"G1","approval_revision":4}` or `{"kind":"price_list","id":"L1","version":4}`. Omit it for live rules. Source IDs/revisions are examples; use confirmed workspace identities. Selection never silently falls back. See [approved pricing sources](pricing-sources.md) for eligibility, exact financial semantics and frozen evidence.

The trusted-local TCP transport supports up to 16 clients and serializes each request separately. Concurrent mutation losers receive revision errors, while simultaneous exact retries commit once. CLI `storage-inspect` and `quarantine-pending NAME SHA256` preserve interrupted-write evidence; see [recovery](recovery.md). Neither command promotes pending data or bypasses committed-state validation.
