# Approved quote booking

M8 closes the converted quote-to-actual gap through one atomic, full booking of the currently approved governed quote. It stores exact approved invoice and cost totals, quantity, currency and unit as actual ledger rows. Ordinary invoice add/CSV import remains catalog-native and cannot manufacture a converted row.

## Eligibility and lifecycle

An Analyst or Admin supplies a new booking ID, quote ID, exact current approved quote version, invoice date and source label. The invoice date must be on or after the quote pricing date. The quote must have a complete, independently approved frozen governance cycle. Legacy quotes require revision and reapproval. Draft, Pending, rejected, stale or previously booked quotes are ineligible.

Every approved line books once in order with IDs `BOOKING-L001` through `BOOKING-L200`. The booking ID is at most 59 characters. Existing invoice IDs and IDs reserved by live correction proposals block the whole command. There are no partial quantities or per-line exclusions. At most 1,000 bookings / 5,000 booked lines fit the domain cap; existing overall snapshot/transaction limits still apply.

A quote ID can be booked only once for its entire lifetime. Revising/reapproving the quote or reversing all its invoices never releases that capacity. An exact original request retry returns the original receipt without another commit; a different request cannot create another booking of that quote. The same command boundary applies in native UI, CLI and JSON API.

## Exact ledger and non-posted native basis

The booking copies the approved cycle, submitted/version/review identities, quote contributors at booking time, conversion and price/contract/cost evidence, booked rows, source label, booking actor and creation revision. Subsequent price, rate, contract or quote changes cannot reprice it. A target-specific audit event binds the immutable booking fingerprint; validation links every row to the booking ingestion revision and retained approval cycle. The current-at-booking check also recognizes exact legacy quote-author audit text, so erasing or redirecting a known revision target cannot hide that mutation. Hashes are local consistency evidence, not authenticated signatures.

The booked totals are the approved line invoice and cost totals. Projected quote rebates, pocket totals and margins are not ledger postings. Actual rebate agreements calculate independently from recorded actuals and approved reversals.

For explanation only, each line also freezes its native currency, unit, normalized quantity, rounded native invoice unit and cost unit, and extended native totals. Native invoice unit is native rule unit minus the rounded negotiated discount; totals extend that unit by native quantity. These amounts are explicitly **non-posted pricing basis**, not another invoice or a relabeled booked amount.

The recorded reconciliation is booked total minus the forward frozen quote-FX conversion of the extended native basis, for invoice and cost separately. UOM is not applied twice. This can be nonzero because the approved converted unit rounds before extension, while the native basis extends before the comparison conversion. For example, a booked EUR 1.77 compared with USD 2.67 at 2/3 yields EUR 1.78 and a −EUR 0.01 difference. No inverse FX rate is invented, and the difference is not an FX gain/loss posting. Quantity and all money operations remain checked exact integer/rational arithmetic.

## Correction, rebates and reporting

A booked invoice permits an independently reviewed **full reversal only**. Arbitrary replacement transaction rows cannot retain its quote provenance and are rejected. To replace the commercial transaction, approve a distinct new quote and book it under a new identity. A multi-line booking may have individual rows fully reversed; partial credits against a row remain unsupported. Original booking and CSV evidence remain immutable after reversal.

Reports in the booked currency use the exact recorded totals. Reporting in another currency requires an explicit direct booked-to-reporting-currency rate for the invoice date, approved before booking. That knowledge cutoff is `booking_revision - 1`, frozen with the report evidence. A rate approved after booking cannot backfill a missing historical rate; the non-posted native basis is never substituted even when its currency matches the requested reporting currency. Reversals cancel the original amounts using the same original rate evidence. Ordinary unbooked invoice reporting retains its existing behavior.

Rebate settlements exclude the frozen source quote contributors as well as the booking actor and other source contributors from Finance approval. Later quote edits do not change those original exclusions.

## Interfaces and exports

Read-only JSON method `preview_quote_booking` accepts a BookingRequest directly. Business command `quote_book` accepts `{request: BookingRequest}` inside the ordinary revisioned/idempotent envelope. Read-only `export_booking {id}` returns immutable CSV. CLI forms:

```text
copperline-cli --data WORKSPACE preview-booking '{"id":"BK-1","quote_id":"Q-4001","quote_version":4,"invoice_date":"2026-10-20","source":"synthetic-approved-quote"}'
copperline-cli --data WORKSPACE execute REQUEST_JSON
copperline-cli --data WORKSPACE export-booking BK-1 NEW_FILE.csv
```

The request JSON for `execute` uses current workspace revision, a unique request ID and local actor, as documented in api.md. Preview validates eligibility and returns exact rows; execution additionally enforces actor role, workspace revision and persistence. The native **Quote bookings** screen uses this preview and command, with a shortcut from an approved quote. It shows immutable evidence and writes CSV to a new path only. The correction screen disables replacements for booked rows.

Booking CSV preserves booked columns plus JSON native basis, signed rounding reconciliation, full approved line and fingerprint. Mixed/booked report CSV uses `recorded_*` columns and `booking_evidence_json`. Reports without booked rows retain their earlier bytes and `native_*` headers. For JSON compatibility, `ConvertedInvoice.native` remains the historical field name but means the exact recorded actual; `booking` identifies converted provenance. Never infer catalog-native denomination from that legacy field name.

## Verification and limits

`scripts/bookings_e2e.py` exercises two-line converted booking, superseded rates/contracts, draft/stale/role/date/reserved-ID errors, retry/reopen, frozen exports, full reversal, rebate reconciliation, late-rate rejection and complete archive restore through real CLI/API processes. Engine invariant tests cover rounding, maximum IDs, overflow, failed durable commits and forged evidence. Storage tests rehash historical corruption and require reopen/backup to reject it without changing bytes. Native UI tests run the same engine workflows.

This is a synthetic local actuals ledger. Booking does not issue a customer invoice, post to ERP, execute FX, pay rebates or deliver a document. Partial booking, credits, shipments, accounting-period closure, authenticated actors and external integration remain gaps. Empty booking maps and absent report fields omit serialization so earlier snapshots, receipts and exports remain compatible.
