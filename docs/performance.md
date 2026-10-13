# Reproducible local workload measurements

The M6 release and frozen M7 release build pass both deterministic synthetic workloads through the real CLI's public JSON-lines API. M7 preserves the exact M6 corpus, committed snapshot bytes, state, exports and receipts. These measurements describe one local macOS arm64 run per binary/profile; they are not enterprise capacity, multi-user throughput or latency guarantees.

The measured M6 binary corresponds to source `b2538ac2b07a6bc5412ef953089f662ffe67dabc` and SHA-256 `08e96793d05d3438eb5a71ec65e0e0449554818ed00a1cc8f748ea5beba5bde1`, matching the preserved [M6 artifact manifest](../evidence/m6-artifact-manifest.json). M7 was measured from the final frozen binary with SHA-256 `074720a755feabd71353d64f884362c2b18e535dfaaecbbdd330ae16f46eed60`, before its source commit. Its summaries explicitly record this precommit provenance and base commit; they do not invent a final source commit ID.

The runner is [scripts/workload.py](../scripts/workload.py). Scrubbed summaries retain every timing statistic, count and compatibility digest: [M6 small](../evidence/workload-m6-small.json), [M6 medium](../evidence/workload-m6-medium.json), [M7 small](../evidence/workload-m7-small.json), [M7 medium](../evidence/workload-m7-medium.json). Raw state, requests, exports and process logs remain in ignored local evidence directories. Earlier M7 candidate runs remain local and do not supply the published comparison.

## Results

Values below are p50 / p95 milliseconds. Full summaries also include minimum, maximum and sample counts, with breakdowns by durable command, phase and native/converted evaluation.

| Operation | M6 small | M7 small | M6 medium | M7 medium |
|---|---:|---:|---:|---:|
| Durable command, all phases | 9.782 / 11.882 | 8.943 / 11.345 | 14.912 / 19.689 | 12.003 / 15.534 |
| Durable customer updates after quotes | 11.359 / 12.088 | 9.964 / 11.167 | 19.372 / 19.780 | 14.777 / 15.747 |
| Price evaluation | 0.094 / 0.157 | 0.091 / 0.163 | 0.083 / 0.118 | 0.082 / 0.108 |
| Exact request retry | 0.027 / 0.037 | 0.026 / 0.035 | 0.017 / 0.025 | 0.013 / 0.022 |
| Warm full-history reopen | 32.433 / 33.206 | 31.338 / 32.614 | 1120.252 / 1134.388 | 1118.168 / 1123.840 |

The final M7 medium sample has approximately 19.5% lower durable-command p50 and 21.1% lower p95 than M6. Reopen is essentially unchanged near 1.12 seconds. These are observations from separate local runs, not causal proof or promised improvements; filesystem synchronization and scheduler variation remain uncontrolled. No result was selected against a timing threshold.

The most visible size-scaling difference remains full-history reopen: about 32 ms for 79 snapshots and 1.12 seconds for 327 snapshots. Evaluation p95 remained below 0.2 ms in these samples. The medium workspace retains about 89.3 MiB of history for a roughly 0.49 MiB current state. This points to retained-history and durable-write work as useful profiling targets, but the benchmark does not isolate CPU validation, serialization, filesystem synchronization or cache costs. Both live data and history grow between profiles, so this contrast is not a controlled estimate of one scaling factor.

M7 archive operations each have **one measured sample per profile**, shown in milliseconds. M6 has no archive API. Both restores passed exact committed-file hashes, full state, approved export bytes and original receipt replay.

| Archive operation | M7 small | M7 medium | Samples per profile |
|---|---:|---:|---:|
| Backup | 127.771 | 4202.010 | 1 |
| Full archive inspection | 42.950 | 1432.521 | 1 |
| Restore to new workspace | 502.963 | 5664.489 | 1 |

Archive sizes are 3,673,672 bytes (small) and 93,722,422 bytes (medium). This workload's archives have no pending/recovery entries; preservation of that evidence is covered by separate reliability regressions.

## Fixed corpus

All dates, IDs, prices, actors, CSV order and command order are deterministic. No random generator, clock-derived business input, real company data, live FX feed or network service participates. The versioned corpus digest covers its specification, commands, evaluation requests, retry selection and expected results.

| Profile detail | Small | Medium |
|---|---:|---:|
| Products / cost inputs | 10 / 10 | 50 / 50 |
| Customers | 3 | 10 |
| Recorded native invoices | 100 | 1,000 |
| Invoice CSV batches / rows per batch | 4 / 25 | 10 / 100 |
| Approved two-line quotes | 5 | 20 |
| Native USD quotes / converted EUR quotes | 3 / 2 | 10 / 10 |
| Later durable customer updates | 20 | 100 |
| Measured durable commands | 78 | 326 |
| Measured evaluations, half native and half converted | 50 | 100 |
| Untimed evaluation warm-ups | 5 | 5 |
| Measured exact retries | 20 | 50 |
| Measured full-history reopens | 3 | 5 |
| Committed snapshots, including revision zero | 79 | 327 |
| Current state JSON bytes | 90,665 | 513,326 |
| Latest stored snapshot bytes | 90,832 | 513,493 |
| Total committed history bytes | 3,661,985 | 93,674,567 |

Product `i` has native USD/each list price `10000 + 137*i` cents and cost `5000 + 61*i` cents. One independently approved customer contract supplies each product at 90% of list, rounded to cents. One independently approved conversion book declares USD→EUR at 9/10 and a product-specific box→each factor of 12. Quotes alternate ten native each units and one converted EUR box, with 1% negotiated discount and 2% rebate. Native invoices are imported separately in batches, with quantities cycling from one through nine and dates within October 2026. Repeated customer updates keep the live catalog bounded while extending committed history.

The small corpus SHA-256 is `3359b3bd829958842dbcf6fc94199bb1f35f5c22fda0b2581dbfeee5ea568ec5`; medium is `5e9007cc9c2ae9958329b5283a81cab594ad808c4d75f773c0b4d9cb35d5bca1`. All four final comparison runs used runner SHA-256 `200b9e7ae4cfc4561b491eb491205d99e1e83fef68a699e15f3b94b7a5ff6e35`. Counts and storage bytes in the table are identical for M6 and M7. A later harness-only change canonicalizes explicit output paths before testing separation; it leaves the measured corpus, timing boundaries and binaries unchanged. Five focused path regressions cover `..`, existing destinations and filesystem aliases where supported. Explicit Windows device/verbatim namespace paths are rejected by this portable harness. The measured runner remains available at source commit `470b12b7a73cc91e022c5817fc3f5549e5f79a96`.

**Quote-to-actual booking remains missing.** The workload's invoices are independent native USD/each actuals, not bookings of its converted EUR/box quotes. Current transaction validation requires the catalog product's native currency and unit. This benchmark does not demonstrate a converted quote→invoice flow; see the [conversion boundary](conversions.md#quote-to-actual-boundary) and [gaps](gaps.md).

## Measurement boundaries and correctness

The runner starts one persistent CLI process for the durable corpus and reads. Timing starts immediately before writing an already encoded JSON request and ends when its complete response arrives. It includes local pipe transfer, CLI work and persistence synchronization for durable commands. Python request encoding, JSON response decoding, independent arithmetic checks and snapshot-size inspection occur outside that interval. Durable timings mix command types and increasing state sizes; use the phase and command breakdowns when comparing changes.

Evaluation uses five explicit untimed warm-ups. Retry measurements resend exact earlier requests after the final durable revision and require the original receipt revision without creating another snapshot. Reopen timing starts before process launch and ends when the first complete state response arrives, before Python JSON decoding. It therefore includes process startup, lock acquisition, full committed-history validation, state serialization and transport. Reopen uses warm OS caches after writing the corpus; the runner never flushes caches. Cold-cache startup, concurrent clients, remote transport, authenticated multi-user operation, long-duration soak and interactive Windows performance have not been measured.

P50 is the sample median; p95 is the nearest-ranked observation. With only three or five reopen samples, p95 is the maximum observed sample and is not a tail guarantee. The response timeout is an operational hang safeguard, configurable with `--timeout`, not a performance acceptance threshold. No timing pass/fail thresholds exist.

Every run checks exact integer financial amounts, approved contract and conversion evidence, invoice/report totals, audit/receipt identities, immutable retry behavior, rejection of changed contents under an existing request ID, full state equality after every reopen, and byte-stable approved quote exports. The repeated small M6 run produced identical corpus, state, export and committed-history hashes. The new-only directory guard was exercised against an existing workspace and left every snapshot unchanged.

Current storage caps each snapshot at 16 MiB. This runner additionally reserves the maximum next snapshot before each mutation and stops before committed history could exceed 512 MiB. These are safety bounds, not validated production capacity. State size is canonical compact sorted-key UTF-8 JSON; snapshot/history sizes are actual stored bytes. The runner does not prune history, bypass validation, remove durability synchronization or alter application limits.

## Reproduce and compare

Run from the repository root with Python 3 and an appropriate native release binary. Every workspace and evidence destination must be new; use different names for another run. The supplied binary is copied into a private temporary directory before execution and only that copy is made executable.

```sh
python3 scripts/workload.py --binary dist/m6-macos/copperline-cli \
  --profile small --output-data data/workload-m6-small \
  --evidence-dir handoff-local/workload-m6-small

python3 scripts/workload.py --binary dist/m6-macos/copperline-cli \
  --profile medium --output-data data/workload-m6-medium \
  --evidence-dir handoff-local/workload-m6-medium
```

For M7, run the identical profiles and compare against the checked-in scrubbed baseline summaries. `--baseline-result` requires the same corpus, full-state digest, export digests and receipt count, reports exact history equality, and calculates descriptive p50/p95 ratios without performance thresholds.

```sh
python3 scripts/workload.py --binary target/release/copperline-cli \
  --profile small --output-data data/workload-m7-small \
  --evidence-dir handoff-local/workload-m7-small \
  --baseline-result evidence/workload-m6-small.json --backup

python3 scripts/workload.py --binary target/release/copperline-cli \
  --profile medium --output-data data/workload-m7-medium \
  --evidence-dir handoff-local/workload-m7-medium \
  --baseline-result evidence/workload-m6-medium.json --backup
```

On Windows use the Windows binary path ending in `.exe` and the corresponding Python command/shell continuation syntax. No Windows timing result is implied by the macOS results.

`--backup` is explicitly opt-in and must not be supplied to M6. It invokes `--data DIR backup ARCHIVE`, `backup-inspect ARCHIVE`, and `restore ARCHIVE NEW_DIR` once each with unique destinations. Each timing includes the command's process lifetime and captured output, while JSON decoding/checks occur afterward. The runner verifies restored committed file names, sizes and hashes, full state, approved exports and original receipt replay. This is a synthetic successful round trip; corruption and interruption handling require the separate reliability tests.

Public workload summaries contain hashes, counts, timing distributions and platform details. They omit workspace paths, executable-copy paths, process IDs, command/state payloads, logs and raw exports. Local raw evidence remains ignored and is never needed to reproduce the public corpus.
