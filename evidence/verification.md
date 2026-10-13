# Verification checkpoint — 2026-10-13

Local Apple Silicon macOS, Rust 1.95.0. Second runnable milestone:

- `cargo xtask ci`: PASS (layer gate, asset attribution, fmt, workspace/all-target clippy -D warnings, workspace tests).
- 42 core unit/integration tests and 4 native UI interaction/rendering tests: PASS. Independent reviewer supplied 27 regression tests and verified all reported fixes.
- `python3 scripts/e2e.py`: PASS, 40 JSON API requests, 21 committed revisions, actual process restart and frozen CSV comparison. Golden USD 900 invoice / 45 rebate / 855 pocket / 255 margin. Expanded flow includes grid approval/recalculation, agreement approval, atomic invoice ingestion, accrual and weighted invoice analytics.
- Native macOS desktop built, launched and screenshot visually inspected. Screenshot capture required permission to access the native window service outside the command sandbox. No host security settings changed.
- Published baseline `6083f03f3df0b361f46df290c8b0fb2f56843dd4`: [Windows/macOS/Linux CI passed](https://github.com/BLTCPT/copperline/actions/runs/38347140721). Each platform ran quality gates, a native release build and CLI process E2E. Windows additionally produced an unsigned portable ZIP. Second-milestone CI results will be recorded after publication. Interactive Windows desktop behavior remains unverified.

Independent review fixes: pending-file exclusive creation; symlink/nonregular snapshot handling; frozen arithmetic, currency/UOM/cost/aggregate validation; approval chronology and actor audit consistency; currency-scoped price lists; disk-failure rollback and reopen idempotency.

Additional review coverage: active grid price preservation, prior approver reauthoring, strict agreement overlap boundaries, accrual inclusion of every eligible invoice available at the source revision, no future invoice inclusion, transaction provenance, duplicate-import rollback, aggregate overflow and original snapshot compatibility.

Evidence: quality-gates.txt, process-e2e.json, workbench.png, grid.png, agreements.png, analytics.png and approvals.png. Screenshots are rendered native macOS windows using clearly synthetic data. No benchmark or full-parity claim.
