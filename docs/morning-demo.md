# Morning demo — ten minutes

This is a controlled synthetic demo, not production software or complete Pricefx parity. All names, prices and commercial terms are invented. Roles are selected locally for demonstration and do not authenticate a person.

## Prepare once

**Windows artifact download:** GitHub downloads an outer artifact ZIP containing `copperline-0.1.0-windows-x64-portable.zip`. First extract that outer ZIP. Verify the inner portable ZIP against the inner size/SHA-256 in [build evidence](../evidence/verification.md), then extract the inner ZIP to a short, writable local folder. Double-click **Start Morning Demo.cmd** inside the resulting folder. On first run it validates/restores the packaged `morning-demo.clbak` into `data\morning-demo`; later runs reopen that same history. No Rust, Python, installation or administrator rights are required. Keep the binaries, archive, notices and docs together. The build is unsigned; no signing or trusted distribution channel is claimed. `Start Copperline.cmd` still opens the smaller catalog-only demo.

**Source checkout:** install the prerequisites in [README](../README.md), build the CLI and desktop, then from the repository root run:

```sh
python3 scripts/demo_seed.py --binary target/release/copperline-cli --credits --data data/morning-demo
target/release/copperline-desktop --data data/morning-demo --view bookings
```

Use `.exe` binary names on Windows. The seed script preserves an existing nonempty workspace, so use a new folder for a repeatable fresh run. The complete prepared fixture is revision 65. It deliberately retains Q-2001 awaiting Finance, contract edition 2 awaiting commercial review, and stale ST-2001 so review and error guidance can be inspected. Close the desktop before opening its directory with another CLI/API process.

## Walkthrough

| Time | Screen and action | What to verify |
|---|---|---|
| 0–1 min | **Catalog & costs**, then **Price workbench** | VLV-100 has USD 100 list / USD 60 cost. With quantity 10, 10% discount and 5% rebate, invoice is USD 900, pocket USD 855 and margin USD 255 (29.82%). Cost/rule dates and precedence are explained. |
| 1–2 min | **Customer price agreements** and **FX & units** | Approved CP-ATLAS edition 1 uses USD 90 each at quantity 10+. FX-UOM-OCT explicitly converts USD→EUR at 9/10 and box→each at 12/1 for October. Edition 2 remains pending and does not silently replace approved terms. |
| 2–3 min | **Quotes**, select Q-4001; then Q-2001 | Q-4001 is approved at EUR 972 for one box, with frozen contract/FX/UOM evidence. Q-2001 remains Pending after commercial review. Choose Jordan / Finance and finish its review to exercise the final stage; its owner cannot approve it. |
| 3–4 min | **Quote bookings**, select BK-EUR-4001 | Q-4001 was booked once at EUR 972; twelve each / USD 1,080 is non-posted native basis. The quote cannot be booked again. Export the booking to a new CSV path. This records local actuals; it does not issue an invoice. |
| 4–5 min | **Quantity credits**, select CN-EUR-4001 | Approved quantity 0.250 box equals three native each. Credit invoice amount is EUR 243, allocated from original frozen totals. An attempted later credit is blocked by the nominal close even if its record date is later. |
| 5–6 min | **Rebate reconciliation**, select A-2026 and ST-CREDIT-TRUEUP | Original USD entitlement 50 fell after a full invoice reversal and quantity credit. Current target is USD 13.50; signed −USD 36.50 true-up leaves zero remaining adjustment. ST-2001 is stale; eligible Finance may reject it, or its owner may withdraw it. It cannot be approved against outdated evidence. No payment occurs. |
| 6–7 min | **Invoice analytics**, reporting section | The newest frozen report, R-EUR-CREDITED, shows EUR 1,336.50 net revenue / EUR 891 net cost across raw USD and booked EUR records. The earlier R-EUR-BOOKED snapshot stays unchanged. New report IDs are suggested; currency and credit status are explicit. |
| 7–8 min | **Invoice period close**, CLOSE-EUR-OCT | October EUR original invoice basis: EUR 972 gross − EUR 243 credit = EUR 729 net. This differs from the multi-currency converted report above by design. The close excludes FX reporting and rebates; it is not an accounting compliance claim. |
| 8–10 min | **Audit history**, then **Backup & restore** | Inspect review authors, source revisions and command history. Create a new `.clbak` outside the workspace, verify it, restore to a new folder under an existing parent and open the restored workspace. It must retain the same records/revisions. Reopen once more and export to a new path. |

The reporting figures above describe the initial fixture. Completing Q-2001 review or rejecting stale ST-2001 changes audit revision but does not change those actuals. Further business mutations can change new calculations. Review messages name local roles; they are not an identity security boundary.

## Expected errors and recovery

- A second process on the same workspace fails with an explicit lock error. Close the first process; do not remove lock evidence manually.
- Existing CSV, backup and restore destination paths are refused. Choose new paths.
- Pending documents cannot export as approved. Quote/list approvals are on the Approvals screen; contracts, conversions, credits, closes and settlements have their own review controls.
- The stale settlement has no valid approval action. Its owner can withdraw it or eligible Finance can reject it; the current recognized target requires no additional proposal.
- Incomplete/corrupt committed history fails closed. Preserve the workspace. After a failed restore, preserve the partial folder and guard; do not delete either to bypass validation.

## Boundaries and next decisions

Booking is not external invoice issuance; settlement is not payment. The nominal close does not freeze FX reporting or rebate calculations. There is no authenticated identity, tenancy, tax/compliance integration, ERP write, model-based optimization or AI assistant. Partial booking, value-only credits, credit reversal/amendment and close reopening are absent. Full-state snapshots constrain scale. Archives are unsigned and unencrypted. Three-OS CI verifies builds and process workflows; interactive Windows UI and abrupt power-loss behavior remain unverified.

Use the [capability matrix](parity.md), [roadmap](roadmap.md), [security boundaries](security.md) and exact [build evidence](../evidence/verification.md) before planning a pilot. The next decision is whether to prioritize an authenticated single-organization pilot, commercial lifecycle gaps, or larger-data storage; none should be described as already production-ready.
