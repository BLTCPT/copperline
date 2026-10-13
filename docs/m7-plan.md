# M7 — recoverable local workspaces

The acceptance target is a portable, fully validated backup and restore into a new workspace, with measured costs at realistic synthetic demo sizes. Earlier workspaces and export evidence must remain intact. No enterprise-scale, power-loss, authentication or migration claim is implied.

## Scope

- One bounded, uncompressed archive with a versioned strict manifest, exact file lengths and SHA-256 checksums. Preserve committed JSON bytes, pending evidence and all supported recovery files/directories. Reject unsupported evidence instead of silently omitting it.
- Verify every committed revision, including its semantic invariants and hash-chain link, both when inspecting and restoring. Preserve state schema and archived bytes; no migration or history truncation.
- Exclusively create a new archive and new restore target. A durable restore guard prevents normal opening of an incomplete destination; failures preserve owned partial files and disclose paths. Existing targets are never replaced, including empty directories.
- Native Backup & restore view and local CLI commands share engine/storage operations. Arbitrary filesystem actions are not exposed over the local JSON transport. Business revisions and receipts do not change during backup.
- Reuse one validated candidate through commit and serialize a borrowed envelope. Keep public direct commits checked and retain full historical validation on open.
- Repeatable release-mode workload measurements against the genuine M6 artifact using identical input, final state, export and receipt hashes. Separate durable commands, pure reads, retries, warm-cache reopen and full backup/inspection/restore. Report counts, p50/p95/min/max and sizes; no timing threshold masquerades as correctness.
- Fault/corruption, no-overwrite, pending preservation, retry/export compatibility and native interaction regressions. Run the full quality gate and all process flows on Windows, macOS and Linux; verify the downloaded Windows package.

## Prioritized next workflow

M6 review identified converted quote-to-actual booking as a missing integration. Actual invoices remain catalog-native. A later coherent milestone should preserve approved quote versions, booked/native totals and quantities, frozen conversion evidence, reversal/replacement behavior and reconciliation rounding before allowing converted invoice booking. Partial credits and accounting-period close remain after that provenance work. M7 must not trade recoverability for a rushed relaxation of financial validation.

## Evidence still required

Final-source quality logs, process reports, archive/fault coverage, matched-corpus benchmark results, native screenshot, independent review, three-OS CI and verified portable artifact. See the completed milestone status and evidence files for actual results rather than treating this plan as completion.
