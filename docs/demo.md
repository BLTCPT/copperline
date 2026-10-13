# Ten-minute demo

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
