# Architecture

Copperline is an engine-first Cargo workspace. It is original Rust source, inspired by GridCraft's playbook rather than derived from its implementation. The full functional parity target is a roadmap, not a current claim.

| Layer | Crate | Responsibility |
|---|---|---|
| 0 | money | Exact cents, thousandths of units, basis points; checked arithmetic and decimal parsing |
| 1 | model | Typed domain records, schema, dates and snapshot invariants |
| 2 | pricing | Pure rule selection, waterfall, guardrails, weighted totals, scenario comparison |
| 2 | storage | Exclusive workspace lock, committed snapshots, checksums, bounded reads |
| 3 | engine | Role checks, commands, optimistic revision checks, idempotency, state transitions, CSV |
| 4 | ui | Independent egui view; every business action calls the engine |
| 5 | desktop / cli | Native window and process entry; stdio/loopback JSON adapters |

`cargo xtask layers` checks internal dependency edges and bans UI libraries below the presentation layer. `unsafe_code` is forbidden; clippy rejects panic/unwrap/expect/todo/unimplemented/unreachable in authored code. Checked arithmetic, bounded input and Result errors are the first defense; command apply additionally catches an escaped dependency panic before persistence. OOM, OS termination and native graphics-driver failures are not recoverable promises.

## Transaction and persistence
A Request contains request_id, expected_revision, local actor and tagged Command. Exact retries return the original receipt. Reusing a key with different content fails. The engine checks role/revision, clones current state, applies/validates the candidate, commits the new snapshot, then swaps memory. Failure before the commit point leaves current memory and durable state unchanged.

Workspace lock is held for the session. Each revision is a full JSON snapshot linked to the previous SHA-256 hash. Write uses exclusive-create `.pending`, flush+sync, then rename to a unique 20-digit `.json` file. A successful rename is the commit point; Unix directory sync is best effort afterward. Open checks all committed revisions, hashes, structure, arithmetic and approval invariants. Corruption fails closed; there is no silent fallback. A stale pending file blocks that revision until `storage-inspect` and checksum-verified `quarantine-pending` preserve its bytes under the exclusive lock. Recovery never promotes or replays pending data; see recovery.md. The chain detects accidental damage; it is not cryptographic protection against an attacker who controls the workspace.

The snapshot limit is 16 MiB, command limit 2 MiB, CSV limit 1 MiB, quote limit 200 lines, catalog 10,000 products. Full history is intentionally simple; large enterprise volumes need a database/event-store adapter and migration plan.

## Financial semantics
USD/EUR/GBP are supported at 2 monetary decimal places; quantity has 3 places in an explicit product unit. Currency and unit cannot change on an existing product. Mixed-currency documents are rejected; explicit dated direct FX/UOM conversion can produce a common quote currency. No conversion is inferred. The UI's catalog editor uses USD; the command API supports all three currencies.

Round half away from zero. Rule adjustments round at unit level, then negotiated discount rounds at unit level. Invoice = rounded invoice unit × quantity. Rebate rounds once against that extended invoice. Pocket = invoice − rebate; margin = pocket − extended cost. Margin percentage is undefined at zero pocket revenue. Aggregate margin is total margin / total pocket revenue, never an average of line percentages. Unit rebate/pocket displays are indicative; extended totals are authoritative.

Effective cost is the latest effective start at or before the pricing date, then highest version. Missing cost fails; explicit zero cost is valid. Among versions of a rule ID that have started, highest version supersedes earlier versions, even if the newer one is expired; expiration does not resurrect a prior version. Eligible rules match currency, product/customer/segment, quantity, inclusive start and exclusive end. Exactly one wins by descending priority, customer/product/segment specificity, effective start, version, lexical ID. Fixed price, list discount and cost markup are supported; markup is not margin.

Frozen quote/list lines retain computed values, rules, cost version and source state revision. Quotes require separate-person approval. Lists have separate calculate, submit, decide and local publish commands. Publishing is only an internal status transition; it does not write to ERP. Quote edits require Draft; revising an approved/rejected quote invalidates current approval while retaining historical decisions and full stored revisions. All quotes require review even if no soft guardrail fires. Hard margin violations block submission.

## Live grid, agreements and invoice ledger

Live grid items persist a candidate, candidate policy/source revision, review history and a separate last-approved evaluation. Recalculation makes a new Draft and preserves the active price. Pending candidates cannot recalculate. Approval replaces the active price; rejection retains it. The active record is an approved local price register. Quotes/scenarios can explicitly select its approval revision or an exact published list version; no implicit grid/list lookup occurs. Replaced grid approvals are archived so frozen source evidence remains verifiable; see pricing-sources.md. Products/customers are immutable identities within an item.

Rebate agreements use immutable customer/currency/date terms and 1–20 increasing retrospective invoice-revenue tiers. The achieved tier applies to the whole eligible invoice total and rounds once. A separate person must approve; approved agreements for the same customer/currency cannot overlap. No renewal/amendment or progressive-tier mode is implied.

Recorded invoice transactions are append-only, keyed by external ID, and carry an explicit source label, currency, UOM, date, quantity, invoice and cost total. An internal ingestion revision map ties each record to the importing audit event. Transaction CSV import is all-or-nothing. Original invoice rows remain nonnegative. Approved correction records implement exact full reversals and optional atomic replacements; no free-form negative transaction is accepted. Synthetic UI invoices are labeled `synthetic-demo`.

Accruals are immutable whole-period snapshots referencing approved agreement version, source revision and eligible transaction IDs. They cannot predate approval or transaction ingestion. Repeated snapshots are alternative calculations at different data revisions, never additive ledger postings. M4 reconciliation derives signed local settlement adjustments from current entitlement minus prior approved adjustments. No payment or ERP posting exists. Invoice analytics group net revenue/cost after approved reversals by customer/currency and calculate weighted invoice margin. They exclude projected quotes and rebate accruals deliberately; labels say invoice margin rather than pocket margin.

New empty M2 maps omit serialization, preserving checksums of M1 snapshots. Compatibility is regression-tested with an M1-shaped committed envelope, then a new M2 command and reopen.

## M3 governance and local concurrency

Quotes have an immutable creator/owner and conservatively retained contributors. New submissions freeze calculated lines, source evidence, guardrails, policy and ordered commercial/Finance stages. The engine enforces ownership before mutations; the model binds cycles/reviews to exact target-specific audit records. Finance escalation compares effective discount and weighted margin by integer cross-products and per-currency pocket value. Pending policies and lines cannot change. Historical M2 snapshots and receipt hashes are tested against independently captured original bytes. See quote-governance.md.

The optional local TCP API accepts at most 16 connections; each has bounded reads and five-second inactivity timeouts. A mutex surrounds each request, not the connection. Optimistic workspace/document revisions and exact idempotency operate across simultaneous callers. The Store still has one writer and requires the whole workspace to fit the snapshot limit. This is not authenticated enterprise multi-user service.

## M4 immutable corrections and recognition

`model::corrections` reconstructs the invoice basis at any recorded source revision. A Finance-approved correction reverses exactly one original and atomically adds an optional replacement. Proposals reserve replacement IDs and preserve immutable original context. Effective record dates do not schedule activation; approved revisions control historical visibility. Rebate eligibility restates the original agreement period while replacements qualify independently.

`model::settlements` derives the current rebate position and validates signed recognition chains. Each proposal pins an accrual and all previously approved settlement IDs. Submission/approval require exact current basis IDs, not merely matching totals or a global workspace revision. Contributor-aware Finance review excludes source authors. Reject/withdraw remain possible when a proposal becomes stale. Creation/review audit details bind canonical immutable-term hashes using pure serialization/SHA-256; this introduces no filesystem, network, UI or clock in the core. Hashes are consistency evidence, not authentication.

`engine::corrections` and `engine::settlements` implement lifecycle commands. CLI, local API and native correction/reconciliation views share those commands. Settlement CSV exports freeze reviewed evidence and never initiate a payment. See corrections-reconciliation.md for arithmetic, date, currency and lifecycle details.

## M5 customer price-contract editions

The pure `model::contracts` module stores immutable family editions, stable price rows and origin evidence. `engine::contracts` supplies create/submit/decide/withdraw commands. Business editions and workflow versions are separate. Resolution selects the latest approved started edition per family before filtering eligibility, then highest priority for explicitly automatic requests. Approval rejects equal-priority overlapping effective intervals/bands. Historical validation filters approvals by capture revision and preserves legacy point-date source bytes.

`quote_replace_line` builds a complete candidate line set and evaluates it once against current state/policy before persistence. Errors preserve all old lines, source evidence, contributor history and durable state. Native contract and replacement forms, CLI/API commands and conditional CSV evidence use the same engine. M5 retained native source authoring; no clock or network was added to the deterministic layers. See customer-price-agreements.md.

## M6 exact direct conversion

`money::Rational` uses checked i128 intermediates, exact thousandths normalization and one combined FX/UOM unit rounding. `model::conversions` owns immutable direct schedules, approval/audit binding and frozen native evidence; `model::reporting` reconstructs recorded actuals at a source revision and freezes explicit reporting-currency views. These pure modules have no clocks, network or storage dependencies. Pricing normalizes quantity before native source qualification, then converts original unit amounts. Engine lifecycle/CSV/batch commands and replaceable native controls share the same invariants.

Atomic multi-line replacement reevaluates the complete candidate before one persistence operation. Historical rate evidence is validated at capture revision and report history at its original source revision. Empty maps/absent fields retain earlier serialization. See [conversions](conversions.md) for rounding, direction, bounds and intentionally deferred conversion cases.

## M7 operational archives and validation evidence

`model::ValidatedState` is an opaque immutable borrow created only after full state validation. Engine execution produces it once; `Store::commit_validated` accepts that borrow while retaining revision, file-size and filesystem checks. Public `Store::commit` still validates its input. Borrowed envelope serialization preserves the original field order and exact bytes without a second full-state clone. No historical open checks were removed; compile-fail and corruption regressions protect that boundary.

`storage::backup` owns a bounded framed archive and guarded new-directory restore. It preserves exact committed and pending/recovery bytes, fully validates each historical snapshot, and refuses unknown evidence rather than silently dropping it. The engine exposes local operational methods for the native view and CLI; these are not business commands and do not create audit revisions. Arbitrary filesystem paths are not accepted over the local JSON API. The restore guard and exclusive target lock prevent normal opening before complete validation. See [backup semantics](backup.md), including failure residue and platform durability limits.

## M8 immutable quote booking

`model::bookings` derives exact booked rows from a frozen approved cycle and validates native non-posted basis, rounding, ordered line membership, unique quote capacity, historical current approval and audit bindings. `engine::bookings` inserts all rows and provenance atomically through ordinary candidate validation and persistence. Raw invoice commands still validate catalog-native currency/unit before insertion. Corrections recognize exact booking membership and permit full reversal only. Reporting pins direct FX knowledge to the booking's original revision; settlement review incorporates the frozen quote contributors. CLI/API/CSV and the independent native booking view use these same interfaces. No additional dependency or IO entered the deterministic layers. See [bookings](bookings.md).


See the M9 quantity-credit extension below for the alternative to full reversal; booked raw replacement remains disallowed.

## M9 quantity credits and nominal close

`model::credits` derives cumulative original-total allocations with exact immutable provenance, booked native evidence, historical capacity and contributor review. `model::periods` reconstructs nominal invoice-date/currency bases and prior close approvals without clocks or recursive state validation. Every ingestion, booking and correction path checks the same historical close boundary. The two engine modules provide immutable lifecycle commands; reporting, analytics, accruals and settlements include credit provenance and reductions. Native views and CLI/API/export remain replaceable consumers. Absent maps/fields preserve older bytes. See [semantics and limits](credits-periods.md).
