# Verification checkpoint — 2026-10-13

Local Apple Silicon macOS, Rust 1.95.0. Initial runnable milestone:

- `cargo xtask ci`: PASS (layer gate, asset attribution, fmt, workspace/all-target clippy -D warnings, workspace tests).
- 28 core unit/integration tests and 3 native UI interaction/rendering tests: PASS. Independent reviewer supplied 15 regressions and verified fixes.
- `python3 scripts/e2e.py`: PASS, 25 JSON API requests, 10 committed revisions, actual process restart and frozen CSV comparison. Golden USD 900 invoice / 45 rebate / 855 pocket / 255 margin.
- Native macOS desktop built, launched and screenshot visually inspected. Screenshot capture required permission to access the native window service outside the command sandbox. No host security settings changed.
- Windows/macOS/Linux CI configuration prepared. Remote Windows build/runtime results will be recorded after publication; local Mac evidence does not verify Windows runtime.

Independent review fixes: pending-file exclusive creation; symlink/nonregular snapshot handling; frozen arithmetic, currency/UOM/cost/aggregate validation; approval chronology and actor audit consistency; currency-scoped price lists; disk-failure rollback and reopen idempotency.

Evidence: quality-gates.txt, process-e2e.json, workbench.png. No benchmark or full-parity claim.
