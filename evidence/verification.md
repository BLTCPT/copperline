# Verification — M9

Local Apple Silicon macOS, Rust 1.95.0. M9 quantity credits and nominal invoice-period close are implemented. Independent reviews and complete local source gates pass. Final-source Windows/macOS/Linux CI artifact verification follows source publication; prior verified M8 evidence remains in [m8-verification.md](m8-verification.md).

Release process checks pass: quote/source 65 calls /32 revisions, corrections72/34, contracts75/30, conversions62/26, bookings116/41, quantity credits110/44, backup39 calls and 13 transport/recovery scenarios. New workflow checks original-total thirds rounding, exact exhaustion, original-rate/native evidence, rebate true-ups, close races, atomic rejection, export/reopen and exact archive restoration.

Genuine M8 compatibility preserves 52 original snapshots /1,267,874 bytes at revision51, entire state, eight CSV exports and original seed receipt. The separate restored history adds EUR243 credit (0.250 box /3 native each) and a nominal October EUR close. Both exact retries and all58 snapshots survive archive restoration. Original M8 binary/workspace are unchanged.

Native macOS screenshots [credits](credits.png) and [nominal close](period-close.png) are visually inspected at review revision65. The demo shows EUR972 gross less EUR243 credit = EUR729 net invoice and EUR486 net cost in the closed nominal scope. No accounting-period compliance assertion or closed-period carry-forward is provided. Interactive Windows UI remains unverified.


Final local `cargo xtask ci` passes layering, attribution, formatting, strict Clippy and 255 total Rust checks (252 runtime /3 compile-fail), including16 native UI tests. M9 adds14 credit engine tests,10 close engine tests,1 independent CSV/integration regression,3 storage tests and4 native tests relative to M8. The storage suite rejects43 rehashed middle-history mutations through reopen and backup while preserving input bytes. Independent reviewers found no remaining integration blocker. The five workload-path tests and small workload/archive drill also pass (100 raw invoices,5 quotes,78 commands,79 snapshots,3,661,985 history bytes); this is regression coverage, not a new performance claim.
