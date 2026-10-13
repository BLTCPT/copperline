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
