# M8 — approved quote booking

The acceptance target is a complete synthetic quote → actual invoice → reversal/reporting flow with immutable commercial evidence. This is local recordkeeping, not an ERP invoice issuance, payment or accounting posting.

## Bounded booking rule

Book all lines of one currently approved governed quote in one atomic command, once per quote ID for its lifetime. Exact quote version, valid invoice date at or after pricing date, an explicit source label and an unused booking ID are required. Only Analyst/Admin may book. Legacy quotes without frozen approval cycles must be revised and independently reapproved. Partial booking, partial credits and booking reuse after reversal are deferred. A replacement commercial transaction requires a distinct approved quote and booking.

Generated invoice row IDs use `BOOKING-L001` through `BOOKING-L200`; booking IDs therefore have a 59-character maximum. Existing transaction IDs and correction reservations block the whole booking. Exact request retries replay the original receipt. A new request cannot duplicate a booked quote, even after later quote revision or reversal.

## Financial evidence

Actual ledger rows contain exact approved booked currency, UOM, quantity, invoice and cost totals. The frozen approval cycle retains the quote pricing date, rule/contract origin, original direct FX/UOM evidence, cost version, commercial policy and reviews. Booking does not evaluate current pricing or rates.

Native pricing evidence is separate and non-posted: apply the negotiated discount to the frozen native rule unit, then extend exact native quantity; extend frozen native cost similarly. Store booked minus forward-FX-converted native totals as an explicit rounding reconciliation. No inverse rate, amount relabeling, second invoice, FX gain/loss posting or projected rebate posting is implied.

Booked-currency reports use exact booked totals, including same-currency UOM rounding differences. Other reporting currencies require a direct invoice-date FX rate approved by the booking's source revision. Newer rates never revalue historical bookings or fill a rate missing when booked. The native pricing basis is never silently substituted. Existing raw-invoice reporting and serialized history remain compatible.

## Corrections and validation

Booked invoice rows permit only the existing independently Finance-approved full reversal. Both command and stored-model validation reject arbitrary replacements, including for a native-currency booked quote. Reversals cancel original booked and reporting amounts exactly. Quote contributors captured when booked remain excluded from downstream settlement review; later quote authors do not change old evidence.

Validate booking identity, full ordered line coverage, immutable approval cycle/version, eligibility at booking time, exact derived rows/native basis, creation audit fingerprint, complete ingestion revisions and permanent quote uniqueness. Preserve empty-field omission so genuine M7 workspaces and exports retain their original bytes.

## Delivery gates

Native preview/book/evidence/export, CLI and JSON command/read/export interfaces; independent financial/history review; rounding, superseded rates/contracts, stale quote, duplicate/reservation, failure atomicity, retry/reopen, correction/reporting and legacy compatibility regressions; complete process workflow. Run the full local gate and Windows/macOS/Linux CI, verify Windows package and macOS artifacts, capture the native screen, and replace the same Library handoff. No deployment or email.
