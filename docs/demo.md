# Demo reference

Start with the [current ten-minute morning walkthrough](morning-demo.md) for the complete prepared workflow. The sections below retain milestone-specific drills and error exercises.

## Basic catalog and quote demo

All products, customers and prices are synthetic. Use a new writable local directory. Native desktop requires Rust 1.95+ to build; `cargo run -p copperline-desktop -- --demo --data data/demo` opens the workbench. Windows portable packages contain the desktop and CLI binaries plus this guide. No installation or administrator rights are needed.

1. In **Price workbench**, keep Atlas Process Systems, 2026-10-13, quantity 10, valve VLV-100, discount 10%, rebate 5%. The list price is USD 100, cost USD 60. Invoice is USD 900, rebate USD 45, pocket revenue USD 855, and margin USD 255 (29.82%).
2. As **Alex / Sales**, create a quote, then add the selected product. Open **Quotes** and inspect the frozen data revision and totals. Submit for approval.
3. Switch the local demo identity to **Riley / Approval**. In **Approvals**, review the quote and decision note; approve it. The quote is now locked. Switch back to Sales to export it to a new CSV path.
4. In **Quotes**, revise the approved quote. It becomes Draft and cannot export as approved until resubmitted and approved again. Old approvals and stored revisions remain available.
5. Switch to **Morgan / Pricing**. In **Catalog & costs**, adjust the valve list price and save it. Add a new cost version with an effective date. Live calculations change; approved snapshots do not. Add a versioned customer rule under **Pricing rules**.
6. In **Price lists**, choose customer/date/quantity and calculate a snapshot. Submit it. Switch to Riley, approve the list. Switch to Morgan and publish it locally. No external system receives a write.
7. In **Scenarios**, compare 10% versus 15% negotiated discount. These are deterministic projections, not sales forecasts or historical actuals.
8. Inspect **Audit history**, close the app, then reopen the same directory. All committed records and approvals persist. Opening another process concurrently returns an explicit lock error.

## Errors worth trying

- Set a discount to 100%: zero pocket revenue has undefined margin and is a hard submission block.
- Change a rule or cost to a date after the quote date: future versions do not apply early.
- Import duplicate product IDs: the entire CSV import fails without partial rows.
- Use the Sales role to change catalog or rules: the engine rejects the action.
- Try to export a pending quote or overwrite an existing CSV path: rejected.
- Close the desktop, corrupt a **copy** of a committed snapshot, then open that copy: checksum/validation failure, never silent fallback. Keep the original data untouched.

## Automated demo

`python3 scripts/e2e.py --binary target/debug/copperline-cli` creates a temporary workspace, executes the quote/list workflow over JSON lines, checks the golden waterfall, verifies failure rollback and restarts the process to verify persistence. It emits evidence to stdout and deletes only its temporary workspace.

## Local toolchain used during development

The build host initially had Rust 1.93.1. A task-isolated Rust 1.95.0 was downloaded under `/tmp/copperline-rustup`, and the direct toolchain bin directory was prepended to PATH. This is an ephemeral development detail; users should install a normal Rust 1.95+ toolchain. Do not depend on that /tmp path for distribution.

## Live grid and rebate demo

1. As Morgan / Pricing, open **Live price grid** and calculate the selected product. Submit the candidate, switch to Riley / Approval and approve/activate it.
2. Change the underlying product price as Morgan and recalculate. The candidate changes while the last approved unit price remains intact. Submit and reject it as Riley: the active price still remains intact.
3. In **Agreements & rebates**, create a retrospective agreement for Atlas, period 2026-10-13 to exclusive 2027-01-01, with 2% base rate and 5% once invoice revenue reaches USD 1,000. Submit and approve with Riley.
4. In **Invoice analytics**, Morgan can record synthetic invoices from the current workbench calculation. Two USD 900 invoices give USD 1,800 eligible revenue. These records are labeled synthetic and do not post to any external system.
5. Return to the approved agreement and calculate an accrual snapshot. At USD 1,800, the retrospective 5% tier produces USD 90 rebate. Add another invoice and calculate a new snapshot: the old snapshot remains unchanged. Do not add snapshots together; each covers the whole period.
6. Invoice analytics shows recorded invoice revenue/cost and weighted invoice margin, excluding quote projections and accruals. This avoids presenting scenario projections as actuals or double counting rebates.

## M3 complete workflow

1. As Casey / Admin, open **Approval policy**. Save 15% maximum effective discount, 25% minimum weighted margin and USD/EUR/GBP 1,000 pocket-value thresholds. These are separate currency amounts, not FX rates.
2. Create and independently approve a grid price for the selected product/customer/date/quantity. Back in **Price workbench**, choose its explicit approval under **Quote / scenario pricing basis**. The approved invoice unit is the new base; the waterfall and source revisions explain the result.
3. As Alex / Sales, create/add a quote and submit it. A sufficiently discounted, low-margin or high-value quote shows two frozen stages. Changing the policy now does not remove the pending Finance stage.
4. As Riley / Approval, approve commercial review. The quote stays Pending and cannot export. As Jordan / Finance, finish the independent second review. Casey / Admin cannot substitute for Finance or edit Alex's quote.
5. Revise and approve a different grid price. The old approved quote/export remains unchanged. Repricing a Draft using the old selected grid approval fails; deliberately remove/replace that line with the new selection.
6. Run `python3 scripts/e2e.py` for the complete process workflow. Run `python3 scripts/transport_e2e.py` for concurrent clients, conflict/retry races and interrupted-write recovery.

For a fully populated synthetic review workspace, run `python3 scripts/demo_seed.py --data data/m3-review`, then open `copperline-desktop --data data/m3-review`. It leaves Q-2001 awaiting Finance. `--source-grid G-VLV-ATLAS` opens the workbench with its approved pricing basis explicitly selected. Existing nonempty workspaces are preserved.

Recovery uses the CLI's checksum-confirmed inspect/quarantine flow in [recovery.md](recovery.md). Keep preserved evidence. Full-period accrual snapshots are alternative calculations; never sum repeated snapshots as payables.

## M4 correction and reconciliation walkthrough

Create a fresh populated workspace with `python3 scripts/demo_seed.py --data data/m4-review`, then open it with the native desktop. The fixture has an initial USD 50 approved local rebate record, an independently approved reversal of the USD 100 invoice, a corrected USD 18 full-period target, and a pending −USD 32 adjustment. Both source invoices remain stored.

1. In **Invoice corrections**, inspect CR-1001: original INV-SYN-002, record date, full reversal, creator and Finance approval. To test replacement, select an active original and enable the replacement option; use a new external ID. Morgan / Pricing creates/submits, Jordan / Finance reviews.
2. In **Invoice analytics**, compare gross and reversed revenue with net actuals. The original record shows its reversal link.
3. In **Rebate reconciliation**, inspect A-2026: gross USD 1,000 minus reversal USD 100 = net USD 900; the retrospective tier falls from 5% to 2%, so target USD 18 minus prior recognized USD 50 gives −USD 32.
4. As Jordan / Finance, approve ST-2001. Recognized rebate becomes USD 18 and remaining adjustment zero. No payment occurs. Export the approved evidence to a new CSV path.
5. As Morgan, calculate another accrual. Creating another settlement for the same unchanged target fails because no adjustment is due. Earlier accrual snapshots must not be added together.
6. For concurrency/provenance cases, run `python3 scripts/reconciliation_e2e.py`. It exercises a +150/−140/−2/−8 sequence to zero, same-person denial, competing proposals, reserved IDs, retry after restart, frozen exports and no-overwrite behavior.

A stale pending proposal can be rejected by eligible Finance or withdrawn by its owner. Calculate a new snapshot/proposal rather than editing a reviewed history record.

## M5 customer contract and quote replacement walkthrough

Run `python3 scripts/demo_seed.py --data data/m6-review`, then `target/debug/copperline-desktop --data data/m6-review --view contracts`. On Windows use the CLI/desktop `.exe` binaries and pass their paths with `--binary` to the seed script. Existing nonempty workspaces are preserved. The M6 fixture is revision 49 and retains the M5 contract workflow, with CP-ATLAS edition 1 approved and edition 2 pending commercial review; Q-3001 retains the edition-1 November price.

1. Open **Customer price agreements**. Edition 1 covers October 1 through exclusive January 1: quantities below 10 use USD 95; quantity 10 and above uses a frozen USD 90 grid invoice unit. Edition 2 starts November 1 and proposes USD 85 for that bulk band.
2. As Riley / Approval, approve edition 2. The automatic November selection changes; October still selects edition 1. Q-3001 and its existing approval cycle remain unchanged.
3. In **Price workbench**, select Atlas, VLV-100, November 15, quantity 10, and **Automatic customer contract**. Inspect the selected edition, row, interval and priority in the waterfall. `--customer-contract` selects this source mode on startup at the default demo date.
4. As Alex / Sales, open Q-3001 and revise it. Expand **Replace a draft line atomically**, set the matching quote date/customer, choose the current contract selection and line, then replace. All retained lines recalculate together. A wrong date or stale retained source leaves the quote intact.
5. Resubmit and complete the frozen review route. Export the approved quote to a new CSV path; it includes contract selection, row, interval, priority and fingerprint. Contract terms have their own approved CSV export.
6. As Morgan / Pricing, expand the contract preparation form. Add fixed price bands or capture an explicitly selected approved grid/list origin. Create an edition, submit and review with Riley. Adjacent intervals are allowed; equal-priority overlapping date/product/quantity coverage is rejected.

Run `python3 scripts/contracts_e2e.py` for boundaries, conflicts, non-resurrection, frozen sources, atomic replacement, restart, retry and export verification. Contract prices never imply FX or unit conversion.

## M6 FX, packaging and reporting walkthrough

1. Build CLI and desktop, then run `python3 scripts/demo_seed.py --data data/m6-review`. New workspaces include approved `FX-UOM-OCT`, approved EUR/box quote `Q-4001`, and frozen October EUR actuals `R-EUR-OCT`; existing directories are preserved.
2. Open `target/debug/copperline-desktop --data data/m6-review --view conversions`. Inspect direct USD→EUR 9/10 in October, 4/5 in November, and VLV-100 box→each 12/1. Each direction/date/ratio is explicit. The book was independently reviewed by Riley.
3. In Price workbench choose automatic customer contracts and enable **Convert quote currency / unit**, EUR and box. Quantity is now boxes. Inspect native quantity, native contract band, exact factors and rounding under the waterfall. One October box resolves 12 each at USD90 each and becomes EUR972 per box before new discounts/rebates.
4. For a new schedule, select Morgan / Pricing, open FX & units, expand **Prepare a conversion book**, and paste the nine-column CSV. Import creates immutable Draft terms; submit as Morgan and approve as Riley. Missing rates, bad directions, inexact quantity normalization or overlapping dates reject without partial changes.
5. In Quotes, choose a Draft and expand replacement controls. Stage each desired line, then apply the batch once. A currency change must cover every line. Existing quote versions and approval cycles remain auditable.
6. In Invoice analytics inspect **Reporting currency · frozen actuals**. R-EUR-OCT includes original invoice rows and approved reversals at original dates/rates. Create another ID to capture later history. Export frozen report to a new path; nothing is posted or paid.

Command examples: `copperline-cli --data data/m6-review export-conversion FX-UOM-OCT conversion.csv`, `export-report R-EUR-OCT report.csv` and `export-quote Q-4001 converted-quote.csv`. Use the full `--data` invocation for each command. `scripts/conversions_e2e.py` verifies the same flow with real restart, exact retry and byte-stable exports. See [conversion semantics](conversions.md) for unit-first rounding and deferred cases.


## M7 backup and recovery drill

1. Keep CSV exports outside the data directory. Open **Backup & restore** in the native app, enter a new `.clbak` path outside the workspace, and choose **Create complete backup**. The verified report includes captured revision, evidence counts, length and checksum.
2. Choose **Verify archive** to re-read all bytes and the complete historical chain. Change the destination to a new folder beneath an existing parent and choose **Restore into new folder**. Existing folders, including empty ones, are refused.
3. Open the restored workspace using the success button. Quotes, frozen conversion evidence, approvals and prior receipts are preserved. The original workspace and archive remain unchanged.
4. For CLI rehearsal, close the native app, then run the three commands in [backup.md](backup.md). `python3 scripts/backup_e2e.py --binary target/debug/copperline-cli` adds corruption, no-overwrite, opaque pending/recovery preservation and an exact retry after quarantine.
5. For measurements, build release CLI and run `python3 scripts/workload.py --binary target/release/copperline-cli --profile small --backup`. The script creates fresh synthetic workspaces and retains evidence. Use `medium` for 1,000 invoices and 20 approved quotes. See [measurement boundaries](performance.md); these are local warm-cache demo measurements.

A failed restore names its preserved partial directory and blocking guard. Keep both, retain the archive, and use another new destination. Never remove a guard to force opening. Windows directory-entry power-loss durability and interactive visual behavior remain explicit limitations.

## M8 converted quote booking walkthrough

1. Run `python3 scripts/demo_seed.py --binary target/release/copperline-cli --data data/m8-review --bookings` against a new directory. Existing nonempty directories are preserved. The optional M8 fixture books approved Q-4001 once as BK-EUR-4001 and adds R-EUR-BOOKED; the default fixture remains compatible with the earlier M6/M7 binaries.
2. Open `target/release/copperline-desktop --data data/m8-review --view bookings`. Inspect the approved EUR 972 / one box invoice, its non-posted USD 1,080 / twelve each basis, frozen quote/conversion evidence and export CSV to a new path.
3. As Morgan / Pricing, select another eligible approved quote, supply new booking ID/date/source and inspect the exact preview before booking all lines. Draft/Pending/stale versions, already booked quotes and occupied/reserved invoice IDs reject atomically.
4. In Invoice corrections select BK-EUR-4001-L001. Replacement is unavailable; create and submit a full reversal as Morgan, then independently approve as Jordan / Finance. Recorded analytics and new EUR reports show its reversal at original evidence. Existing report and booking exports remain unchanged.
5. Returning to Quote bookings never makes Q-4001 eligible again. Replacement business requires a distinct new approved quote. A missing direct reporting rate known before booking fails explicitly; current rates and native basis cannot substitute.
6. Run `python3 scripts/bookings_e2e.py --binary target/release/copperline-cli` for converted multi-line cases, superseded contract/rate history, signed rounding, repeat/reopen/export, corrections/rebates and backup restoration.

The ledger is synthetic and local. It does not issue or deliver invoices or make external postings.


## M9 quantity credits and nominal close walkthrough

Create a fresh fixture with `python3 scripts/demo_seed.py --credits --data data/m9-review`, using `--binary` for a release/Windows CLI. The flag includes the M8 EUR booking, approved quarter-quantity credits on a USD raw invoice and EUR booked invoice, a rebate true-up, a frozen credited report and approved October EUR nominal close. Existing workspaces remain untouched.

1. Open `copperline-desktop --data data/m9-review --view credits`. Inspect CN-EUR-4001: 0.250 box returns exactly 3 native each; the original booked/native evidence remains visible. Select Morgan / Pricing to preview a new USD remaining-quantity credit under a new ID, submit, then use Jordan / Finance to review. Original authors cannot approve their own evidence.
2. Inspect invoice analytics and frozen R-EUR-CREDITED. Credit totals reduce original-period nominal and reporting amounts; earlier R-EUR-BOOKED stays frozen. Rebate reconciliation includes credit IDs and the signed ST-CREDIT-TRUEUP. The earlier pending settlement may now be stale and can be rejected or withdrawn.
3. Open `--view period-close`. CLOSE-EUR-OCT freezes October EUR original invoice/credit IDs and net amounts. Trying another credit on that original fails even with a later record date. It has no next-period carry-forward. USD remains a separate scope.
4. A new close proposal refuses unresolved affected correction/credit proposals. Once proposed, any later invoice or credit change makes its exact basis stale; withdraw and recreate. Closing an empty interval is permitted. Adjacent intervals do not overlap.
5. Export approved credit/close evidence into new CSV paths, close the app, reopen and compare. The local API and CLI use the same commands. Run `python3 scripts/credits_e2e.py --binary target/debug/copperline-cli` for a complete process drill with thirds rounding, exact exhaustion, close races and archive restoration.

The close is explicitly a nominal invoice-basis control. Reporting FX and rebates remain separately evidenced calculations. No accounting compliance, external credit note, payment or ERP posting is implied.
