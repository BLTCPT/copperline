# M9 design: quantity credits and nominal invoice-period close

This bounded milestone adds immutable partial credits and a conservative local close. It is a commercial invoice-basis control, not an accounting-period assertion, inventory valuation, payment, tax treatment, or GAAP/IFRS compliance claim.

## Credit allocation

A credit names one existing invoice and a positive quantity in its recorded unit. It cannot supply arbitrary amounts. Let the original quantity be Q, its nonnegative invoice or cost total be T, prior approved credited quantity be C, and the requested quantity be q. Allocate `round_half_away(T*(C+q)/Q) - round_half_away(T*C/Q)` independently for invoice and cost, using checked integer intermediates. Require `0 < q <= Q-C`. Positive-quantity zero-cent credits are allowed. Final exhaustion clears every original cent. For Q=3.000, invoice=2 cents and cost=1 cent, three credits of 1.000 allocate invoice [1,0,1] and cost [0,1,0].

Freeze the complete original invoice, ingestion revision, ordered prior approved credit IDs, prior quantity, allocation convention, contributor exclusions and optional original booking fingerprint/native evidence. Booked native quantity must normalize to exact thousandths; reject sub-thousandth native returns. Native amounts are non-posted evidence allocated from original native totals. Reporting allocates from the already-converted whole original invoice/cost, never FX-converts each credit independently. Booked rows retain the pre-booking FX knowledge cutoff; raw rows retain the report's historical source revision. Later rates cannot repair a booked row's missing historical reporting rate.

One Draft/Pending credit reserves an original at a time. Approved credits consume quantity but allow a subsequent remaining-quantity credit. Any live or approved full correction blocks credits. Any Draft/Pending or approved credit blocks a full correction. Rejected/withdrawn proposals release reservations; later release cannot hide an earlier overlap. Rejection/withdrawal remain possible when a proposal becomes stale.

Analyst/Admin owners create, submit and withdraw. Independent Finance decides; owner, original ingestion/replacement author, frozen booking/quote contributors and prior approved credit authors cannot decide. Creation and decision fingerprints bind evidence and review. No amendments, value-only credits, credit reversals, partial booking, replacement booking or mixing full reversal with partial credits are supported.

Approval revision activates a credit. Its record date must not precede the original invoice, but does not schedule activation or assert a posting date. Analytics separately show credit-note count, fully credited originals and credited amounts. Rebate eligibility restates the original invoice/agreement period; frozen accruals remain unchanged, and new settlement true-ups compare exact credit IDs, including zero-cent credits.

## Nominal invoice-period close

A close covers a half-open original invoice-date interval [start,end) in one recorded currency, globally across customers. It freezes exact original invoice, approved correction and approved credit IDs, plus gross/reversed/credited/net invoice and cost totals at its source revision. Empty intervals may close. Analyst/Admin owner proposes/submits; independent Finance approves. No reopen or next-period carry-forward exists in this milestone.

Submission and approval require an unchanged exact basis, no affected unresolved credit/full-correction proposals, and no overlapping approved close in that currency. Adjacent intervals and other currencies remain independent. Both original and replacement legs determine whether a full correction affects the interval. After close approval, reject ordinary/CSV/quoted booking ingestion into that interval and credits/corrections affecting it, regardless of their later record date. Multi-row commands fail atomically.

Historical validation reconstructs reservations and closed intervals at each action's revision. A later close must not invalidate legitimate earlier activity; a later rejection cannot conceal an unresolved proposal at close approval. Rebate accrual/recognition and reporting FX are explicitly outside this nominal close; these can change through separately evidenced calculations without changing the frozen nominal invoice basis.

## Acceptance and compatibility

Exercise thirds and half-cent allocations, exact exhaustion, native-unit precision failures, contributor exclusion, stale/rejected proposals, overlapping historical reservations, close races, zero-value new invoice provenance, replacement legs, adjacent boundaries, multi-line booking rollback, reporting original-rate allocation, rebate true-ups, retry/reopen/export/archive and rehashed corrupt snapshots. Preserve absent M9 fields in genuine M8 histories and frozen reports/accruals. Deliver typed API, CLI, native controls and CSV evidence. Full local gates and final-source Windows/macOS/Linux CI precede publication evidence; macOS development does not verify interactive Windows behavior.
