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

Workspace lock is held for the session. Each revision is a full JSON snapshot linked to the previous SHA-256 hash. Write uses exclusive-create `.pending`, flush+sync, then rename to a unique 20-digit `.json` file. A successful rename is the commit point; Unix directory sync is best effort afterward. Open checks all committed revisions, hashes, structure, arithmetic and approval invariants. Corruption fails closed; there is no silent fallback. A stale pending file blocks that revision until deliberately moved aside after inspection. No crash-recovery tool is supplied yet. The chain detects accidental damage; it is not cryptographic protection against an attacker who controls the workspace.

The snapshot limit is 16 MiB, command limit 2 MiB, CSV limit 1 MiB, quote limit 200 lines, catalog 10,000 products. Full history is intentionally simple; large enterprise volumes need a database/event-store adapter and migration plan.

## Financial semantics
USD/EUR/GBP are supported at 2 monetary decimal places; quantity has 3 places in an explicit product unit. Currency and unit cannot change on an existing product. Cross-currency documents are rejected; there is no inferred FX or conversion. The UI's catalog editor uses USD; the command API supports all three currencies.

Round half away from zero. Rule adjustments round at unit level, then negotiated discount rounds at unit level. Invoice = rounded invoice unit × quantity. Rebate rounds once against that extended invoice. Pocket = invoice − rebate; margin = pocket − extended cost. Margin percentage is undefined at zero pocket revenue. Aggregate margin is total margin / total pocket revenue, never an average of line percentages. Unit rebate/pocket displays are indicative; extended totals are authoritative.

Effective cost is the latest effective start at or before the pricing date, then highest version. Missing cost fails; explicit zero cost is valid. Among versions of a rule ID that have started, highest version supersedes earlier versions, even if the newer one is expired; expiration does not resurrect a prior version. Eligible rules match currency, product/customer/segment, quantity, inclusive start and exclusive end. Exactly one wins by descending priority, customer/product/segment specificity, effective start, version, lexical ID. Fixed price, list discount and cost markup are supported; markup is not margin.

Frozen quote/list lines retain computed values, rules, cost version and source state revision. Quotes require separate-person approval. Lists have separate calculate, submit, decide and local publish commands. Publishing is only an internal status transition; it does not write to ERP. Quote edits require Draft; revising an approved/rejected quote invalidates current approval while retaining historical decisions and full stored revisions. All quotes require review even if no soft guardrail fires. Hard margin violations block submission.

## Live grid, agreements and invoice ledger

Live grid items persist a candidate, candidate policy/source revision, review history and a separate last-approved evaluation. Recalculation makes a new Draft and preserves the active price. Pending candidates cannot recalculate. Approval replaces the active price; rejection retains it. The active record is an approved local price register; quote evaluation still explicitly runs current pricing rules, not an implicit grid lookup. Products/customers are immutable identities within an item.

Rebate agreements use immutable customer/currency/date terms and 1–20 increasing retrospective invoice-revenue tiers. The achieved tier applies to the whole eligible invoice total and rounds once. A separate person must approve; approved agreements for the same customer/currency cannot overlap. No renewal/amendment or progressive-tier mode is implied.

Recorded invoice transactions are append-only, keyed by external ID, and carry an explicit source label, currency, UOM, date, quantity, invoice and cost total. An internal ingestion revision map ties each record to the importing audit event. Transaction CSV import is all-or-nothing. Negative credit memos are not supported. Synthetic UI invoices are labeled `synthetic-demo`.

Accruals are immutable whole-period snapshots referencing approved agreement version, source revision and eligible transaction IDs. They cannot predate approval or transaction ingestion. Repeated snapshots are alternative calculations at different data revisions, never additive ledger postings. No settlement, reconciliation, payment or ERP posting exists. Invoice analytics group recorded revenue/cost by customer/currency and calculate weighted invoice margin. They exclude projected quotes and rebate accruals deliberately; labels say invoice margin rather than pocket margin.

New empty M2 maps omit serialization, preserving checksums of M1 snapshots. Compatibility is regression-tested with an M1-shaped committed envelope, then a new M2 command and reopen.
