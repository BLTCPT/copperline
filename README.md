# Copperline

An independent Rust pricing workbench for synthetic industrial data. Calculate an explained price, freeze it into a quote or price list, route approval, persist every change and export approved values.

**Early functional prototype.** Full pricing-platform parity is the long-term target, not a current claim. No affiliation with Pricefx, GridCraft, ArtCraft or any industrial company. No proprietary source, company data, vendor artwork or authenticated vendor materials are included.

## Run

Install Rust **1.95.0 or newer**. From the project directory:

```sh
cargo run -p copperline-desktop -- --demo --data data/demo
cargo run -p copperline-cli -- --data data/cli-demo demo
cargo run -p copperline-cli -- --data data/cli-demo api
cargo xtask ci
```

The desktop uses a native Rust egui window with no browser or Node runtime. Windows, macOS and Linux build jobs are provided; see evidence/verification.md for what has actually run. Close one process before opening the same data directory in another.

## What works

- Product/customer catalog and effective-dated cost inputs; atomic CSV product ingestion.
- Exact cents, basis points and thousandths of units; explicit currency and no floating-point pricing.
- Versioned, effective-dated single-winner rules: fixed price, list discount or cost markup, with documented precedence.
- Live evaluation and an explained list → invoice → rebate → pocket → cost → margin waterfall.
- Frozen price lists with whole-list approval and separate local publication.
- Independently approved customer price agreements add half-open validity periods, quantity bands, explicit priority and frozen source origins.
- Independently approved dated FX and product-specific UOM schedules use exact rational factors, native quantity bands and frozen rounding evidence.
- Explicit reporting-currency actuals preserve original invoice-date rates, reversals and immutable CSV snapshots.
- Atomic single- or multi-line quote replacement recalculates the full Draft transactionally and preserves approval history.
- Explicit approved grid/list prices feed quotes with pinned source evidence; replacement preserves prior quotes.
- Persistent live-grid candidates and last-approved prices that survive recalculation and rejection.
- Approved retrospective rebate agreements, atomic invoice CSV ingestion, frozen accrual snapshots and net invoice analytics.
- Independently reviewed full invoice reversals and atomic replacements preserve original data and restate eligible rebates.
- Reconciliation creates signed local settlement adjustments with source/contributor evidence, stale-basis checks and frozen CSV exports; no payments.
- Owner-controlled quotes, retained contributor exclusions, frozen commercial/Finance routes, exact thresholds, revision and approved CSV export.
- Deterministic scenario comparison and weighted aggregate margins.
- Durable full-state revisions with exclusive locks, checksum validation, audit records and idempotent commands.
- One command engine behind native UI, CLI, stdio and a bounded concurrent loopback JSON API.
- Interrupted-write inspection and checksum-verified quarantine preserve evidence before retry.
- Full approved-quote booking preserves exact converted actuals, native non-posted basis and original-evidence full reversals.
- Independently reviewed quantity credits use cumulative original-total rounding, native/FX provenance and rebate true-ups.
- Finance-approved nominal invoice-period closes block backdated ingestion and original-period corrections without claiming accounting compliance.
- Portable exact-history backups and validated restore into a new workspace preserve pending/recovery evidence without overwriting existing data.

[Demo runbook](docs/demo.md) · [API contract](docs/api.md) · [Architecture](docs/architecture.md) · [Security boundaries](docs/security.md) · [Capability/parity matrix](docs/parity.md) · [Quote governance](docs/quote-governance.md) · [Approved pricing sources](docs/pricing-sources.md) · [Customer price agreements](docs/customer-price-agreements.md) · [FX and units](docs/conversions.md) · [Quote bookings](docs/bookings.md) · [Credits and nominal close](docs/credits-periods.md) · [Backup & restore](docs/backup.md) · [Measured workloads](docs/performance.md) · [Recovery](docs/recovery.md) · [Corrections and reconciliation](docs/corrections-reconciliation.md) · [Remaining gaps](docs/gaps.md) · [Next milestones](docs/roadmap.md) · [License](LICENSE)

## Development playbook

The workspace follows GridCraft's engine-first, swappable-UI, command-driven and never-crash approach. All authored code forbids unsafe Rust, and clippy rejects panics and unchecked unwraps. `cargo xtask ci` runs layering/asset checks, format, clippy and tests. The original source and UI are MIT licensed; third-party dependencies retain their licenses. See [NOTICE](NOTICE).

Publication of this source repository does not constitute a production service or an authenticated business system. Demo role labels are explicitly caller-selected. There are no ERP writes, payments, telemetry or model-based optimization claims.

![Copperline native workbench with synthetic pricing](evidence/workbench.png)
