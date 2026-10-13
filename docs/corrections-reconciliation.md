# Corrections, rebate reconciliation and local settlement records

All examples and native demo records are synthetic. This workflow changes the local ledger and local recognition records. It never sends payments, posts to ERP, delivers customer credit notes or confirms that money moved.

## Immutable invoice correction

An invoice is never edited or deleted. An Analyst/Admin creates a correction proposal that names one original invoice, a reason, a correction record date and an optional complete replacement invoice. Creation freezes the original invoice and proposed replacement. The owner submits it; a different Finance identity approves or rejects. Admin cannot substitute for Finance. The owner may withdraw a Draft/Pending proposal, including one that has become unusable. Terminal proposals do not reopen.

Approval performs one atomic operation: the original invoice becomes fully reversed for net actuals, and any replacement is inserted as a new positive invoice. Reversal amounts, cost, quantity, currency, product and customer come from the immutable original. There are no user-supplied negative invoice amounts. This correction command performs full reversals; a separate Finance-reviewed quantity-credit workflow supports partial returns. Live or approved credits and full correction proposals on the same original are mutually exclusive. A raw-invoice price correction is represented as full reversal plus a corrected replacement.

`effective_date` on the wire means **correction record date**, not delayed activation. It must be on/after the original and replacement invoice dates and the preceding correction record date in a chain. Approval activates the correction immediately at its audit revision. It restates the original invoice period even when the correction was recorded in a later year. No wall clock or future scheduler is implied.

Only one live Draft/Pending/Approved correction may name an original. Rejected/withdrawn proposals release their reservations. Replacement IDs are reserved while a proposal is live and cannot be inserted through ordinary invoice entry/CSV. Approval rechecks uniqueness and commits the reversal/replacement together. A replacement can later be corrected, but links must move forward through committed ingestion revisions; cycles and double reversals are rejected.

A replacement may explicitly correct customer, product, date, currency or unit through the command API, subject to normal invoice context validation. The UI preserves customer/product/currency/unit and edits quantity and values. Cross-currency replacement reverses the old currency and creates a separate new-currency invoice; there is no exchange-rate conversion or combined currency total.

## Actuals and accrual basis

Invoice analytics exposes gross counts/revenue/cost, approved reversal counts/revenue/cost, and net active count/revenue/cost/margin. Zero net revenue has undefined margin percentage. Quote projections, accrual snapshots and settlement records are excluded from invoice actuals.

An accrual snapshot includes every qualifying invoice known at its source revision and every then-approved reversal whose **original** invoice belongs to the agreement. Replacements qualify independently by their own customer/currency/date and ingestion revision. Sorted invoice, correction and approved quantity-credit IDs are frozen. Net eligible revenue equals gross eligible invoice revenue minus full reversals and approved credits. Retrospective tiers apply to the resulting whole-period net revenue and round once. Zero-net accrual is valid when gross invoices were fully reversed.

Older accruals remain unchanged and auditable. Accrual snapshots are alternative full-period calculations; never sum them as liabilities. New fields omit empty values so authentic M1/M2/M3 snapshots and receipt fingerprints remain compatible.

## Settlement reconciliation

A local settlement proposal selects a current accrual snapshot and captures the exact previous approved settlement IDs. Its signed adjustment is:

`full-period target rebate − sum(prior approved local settlement adjustments)`

The prior records are additive adjustments. The accrual snapshots are not. A zero adjustment is rejected. Negative adjustments reduce previously recognized rebate; they are not refunds sent to a bank.

Example: USD 3,000 eligible revenue at a retrospective 5% tier gives USD 150 target and initial adjustment +150. A full reversal leaves USD 500 eligible at a 2% tier, so the new target is USD 10 and the next adjustment is −140. Correcting that USD 500 invoice to USD 400 gives a target of USD 8 and adjustment −2. Reversing the replacement gives target zero and adjustment −8. Approved adjustments sum to zero; all original invoices, snapshots and prior recognition records remain intact.

Settlement lifecycle is Draft → owner submission → independent Finance approval/rejection. Owner withdrawal is permitted only from Draft/Pending. Finance review excludes the owner, selected accrual author, agreement author, eligible invoice authors and relevant correction creators. A generated replacement belongs to its correction creator for this purpose, not to the independent reviewer who authorized its insertion. These are domain controls over caller-selected demo names, not authentication.

Both submission and approval compare exact current invoice/correction/credit IDs and exact prior approved settlement IDs. Same totals with different provenance are stale. A competing proposal cannot recognize the same entitlement twice. Unrelated catalog writes do not invalidate an unchanged basis. Stale Pending proposals can still be rejected or withdrawn; calculate a new accrual/proposal to proceed.

## Persistence and audit evidence

Creation audit events bind a SHA-256 fingerprint of immutable correction/settlement terms. Review events bind the creation anchor and complete reviewer/outcome/note/revision. Model validation also checks target identity, chronology, roles, lifecycle, original/replacement links, exact historical accrual basis, prior adjustment chains and all source contributors. Removing a record while its target audit remains is invalid. Fingerprints and snapshot hashes detect inconsistent state; they do not authenticate actors or resist an attacker who can rewrite the entire workspace.

Engine commands clone, apply, validate, persist and then swap state. Failed validation, stale revision or failed disk commit leaves current memory and durable state unchanged. Exact command retries replay their prior receipt, including after restart.

Approved settlement CSV uses frozen evidence and includes source/accrual/prior-record IDs, signed amounts and Finance review. Text is formula-escaped; typed negative monetary values remain numeric decimal fields. Existing export paths are never overwritten. No editable current catalog labels are incorporated into historical exports.

## Limits and deferred work

Full reversals, raw-invoice atomic replacements and separate quantity credits are implemented. Credit-note delivery, value-only credits, credit amendments/reversals, tax/freight corrections, accounting close/reopen, agreement amendments and actual payment/ERP reconciliation remain gaps. Explicit approved FX/UOM and reporting conversions exist, but raw correction replacement does not implicitly convert values. Nominal invoice-period close is bounded separately and does not freeze reporting FX or rebate calculations. Settlement approval recognizes a local obligation only. No Paid status or external transaction acknowledgement exists. Full-state snapshot size and single-writer limits remain in force.

Booked quote rows permit either exact full reversal or the mutually exclusive M9 quantity-credit workflow. Raw replacement is rejected because it cannot preserve an approved quote's frozen provenance. A replacement commercial transaction requires a distinct approved quote and booking. Each line may be fully reversed independently, but the source quote remains permanently consumed. Reporting reversals cancel exact recorded amounts at the original booking-time FX evidence; native pricing basis is not a posting. See [bookings](bookings.md).
