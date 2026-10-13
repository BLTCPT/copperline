# Portable backup and safe restore

Backups preserve the entire supported workspace record: every committed JSON snapshot, original audit/approval/retry history, root pending files, and regular files and directories under `recovery/`. Snapshot and evidence bytes are copied exactly. The operational `workspace.lock` is excluded and recreated at the destination. Backup/restore does not alter business revisions, approve a quote, replay a command or promote an interrupted write.

## Use

Close the desktop/API before backing up its workspace from another process. The native **Backup & restore** view can back up the currently open workspace using its existing exclusive lock.

```text
copperline-cli --data data/demo backup demo-history.clbak
copperline-cli backup-inspect demo-history.clbak
copperline-cli restore demo-history.clbak data/restored-demo
copperline-cli --data data/restored-demo state
```

The archive path must be new and outside the source workspace. The source must already exist and contain committed history. Inspection is read-only and validates the whole archive and every historical snapshot. Restore requires a **new folder in an existing parent directory**. Existing files, empty folders, workspaces and symlinks are refused. An archive can be inspected/restored without opening or initializing the CLI's default workspace. Success responses are JSON; errors go to stderr with a nonzero exit status.

In the native view, enter an archive path and choose **Create complete backup** or **Verify archive**. The report shows the captured revision, snapshot/evidence counts, whole-archive length and SHA-256. Enter a new destination and choose **Restore into new folder**. After success, **Open restored workspace** switches to that verified copy. The original stays intact. Verification describes the bytes read at that time; restore verifies them again. Filesystem actions are deliberately local UI/CLI operations, outside the JSON business-command transport.

Store approved CSV exports outside the workspace and preserve those files separately. Backup rejects unsupported root files instead of silently skipping them. It also refuses symlinks, nonregular evidence, unsupported paths and bounds violations. It never edits or removes those source files. Pending and recovery files are opaque evidence; they need not be complete or valid snapshots.

## Archive contract and limits

Format 1 is uncompressed: a fixed 16-byte magic, little-endian 64-bit manifest length, 32-byte SHA-256 of the manifest, strict UTF-8 JSON manifest, file bodies in manifest order, a fixed 16-byte end marker, and a 32-byte checksum of the preceding body. The response also includes SHA-256 of the **complete archive including the footer**. Unknown manifest fields, wrong format/schema versions, duplicate or unsorted paths, absent parents, case collisions, checksum mismatches, length mismatches and trailing bytes reject.

The manifest declares state schema 1, committed revision and hash-chain head, plus each member's relative path, kind, length and SHA-256. Every canonical 20-digit committed filename from revision zero to the declared head is required. Inspection validates canonical state hashes and every revision's financial, approval and structural invariants; a valid latest snapshot does not excuse an invalid older snapshot. Restore also verifies the exact copied files before making the destination available. No migration, truncation, reserialization or history repair occurs.

Limits are 512 MiB per complete archive, 16 MiB per file, 2 MiB per manifest and 10,000 entries including directories. Portable member paths are at most 240 ASCII bytes and eight components deep. Each component is 1–128 characters using letters, digits, dot, underscore or hyphen; dot/dot-dot, trailing dots and Windows device names are rejected. The new restore folder's basename has the same portable component restrictions. Its existing parent may have a Unicode name. Use a short local destination path: host filesystem path and space limits still apply.

An unsupported or too-large workspace fails the entire backup; it is never partially included under a successful result. Current full-snapshot storage has other domain limits and validates all history on open. See [measured workloads](performance.md) before extrapolating beyond demo sizes.

## Interrupted operations

A failed archive write can leave a new incomplete archive at the reported path; its original workspace is untouched. Do not call that file a usable backup until `backup-inspect` succeeds. A retry uses another new archive path.

Restore first validates the archive, then exclusively creates a sibling `.copperline-restore-<hash>.guard` containing the target and archive identity. It syncs that guard before creating the destination. Normal workspace opening checks guards on the target and its ancestors before creating directories and again after acquiring the workspace lock. This prevents a partial restore from becoming a new empty or incomplete active workspace, including through resolved filesystem aliases. The restore holds the destination lock while copying, syncing and validating every member. Removing the guard is the application commit point.

If failure occurs before the restore owns a destination, only its own verified guard may be removed. If failure occurs after destination creation, both the partial directory and guard are preserved and named in the error. **Preserve them; do not remove the guard to force opening.** Keep the original archive and restore into a different new folder. M7 does not offer a resume, arbitrary guard removal or automatic cleanup command. Other existing directories are never removed or replaced.

After a successful restore, an exactly-next pending file still blocks the next business write. Use the [existing inspect/quarantine recovery flow](recovery.md) to preserve it, then retry the original command with its original request identity. A committed receipt replays once; an uncommitted candidate remains uncommitted evidence.

## Durability and trust

Files are synced. Unix directory entries are synced before removing a restore guard; a failure keeps the restore guarded. A sync failure after the commit point returns success with a warning and the completed path. Windows directory-entry sync is unavailable in this implementation and is reported as a warning: process-interruption behavior is tested, but abrupt power-loss durability is not confirmed. No physical power-loss tests or network-filesystem guarantees are claimed.

SHA-256 establishes consistency and detects accidental damage; an attacker controlling the archive can recompute it. Archives are neither encrypted nor digitally signed. Filesystem permissions and sufficient disk space remain the local account's responsibility. Concurrent malicious directory replacement is outside the trusted-local boundary. Corrupt committed history fails closed; preserve it for investigation rather than deleting revisions to obtain a successful backup.
