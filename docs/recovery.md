# Interrupted-write recovery

Copperline verifies every committed snapshot when opening a workspace. A crash before the final rename can leave `00000000000000000001.pending` beside committed revision zero. This blocks another write to revision one. Recovery preserves those bytes for inspection and allows the original command to be retried; it never approves, promotes or replays the pending data.

Close the desktop and any CLI/API process using the workspace. Use the same data directory for both commands:

```text
copperline-cli --data data/demo storage-inspect
copperline-cli --data data/demo quarantine-pending 00000000000000000001.pending SHA256_FROM_INSPECTION
```

Use the exact filename and SHA-256 from the inspection response. The example revision is illustrative; use `next_revision` from your own report. The inspection response lists file type, size, checksum, whether the name is exactly the next pending revision, and whether its bytes form a complete candidate. Both incomplete and complete candidates remain uncommitted evidence.

The quarantine command holds the exclusive workspace lock, verifies the committed chain and the supplied byte checksum, syncs the inspected file, then moves exactly that pending file into a newly created subdirectory of `data/demo/recovery/`. A file-sync failure leaves the original path intact. It returns `preserved_path`, `sha256`, `next_revision` and any durability warnings. Every recovery attempt uses a fresh directory; earlier evidence is never overwritten. A directory-sync warning after the move includes the preserved location and is returned as a successful preservation with a warning, rather than an ambiguous failed operation. Directory sync is unavailable on Windows and reported explicitly.

Afterward, reopen the workspace and retry the original command with its original request ID and expected revision. A command that was committed before a lost response returns its original receipt on exact retry. A command represented only by the quarantined pending file was never committed and can run normally. Neither case duplicates a business mutation.

Recovery refuses unknown filenames, future or earlier pending revisions, symlinks, directories, files larger than 16 MiB, stale checksums, symlinked recovery directories and workspaces whose committed history is corrupt. Inspection reports unsupported pending evidence without following symlinks. Such evidence stays untouched. Preserve the entire workspace for manual investigation; do not delete committed revisions or change checksums to force opening.

The tool does not inspect other directories, change access controls, provide backups or defend against a malicious process modifying files inside the same directory. Workspace protection still relies on the host account and an ordinary local filesystem. Keep recovery evidence with the workspace until it has been independently reviewed and backed up.

## Local connection behavior

The opt-in `serve PORT` transport binds only `127.0.0.1`. It supports at most 16 simultaneous connections, with 5-second read/write inactivity timeouts and a 2 MiB line limit. Socket reads and writes occur outside the engine mutex. The mutex serializes individual requests, so two clients may remain connected and work independently, but competing mutations must supply the current workspace revision. Identical simultaneous retries commit once and share the same receipt. Excess clients receive a best-effort busy response and must reconnect later. An idle timeout is not an authentication boundary or a total connection lifetime limit.

Local actor names and roles remain caller-selected demo identities. Concurrent transport support does not provide remote authentication, tenancy, SSO or impersonation resistance.

`python3 scripts/transport_e2e.py --binary PATH_TO_CLI` exercises actual loopback clients, optimistic conflicts, replay races, resource limits, process restart and the recovery commands using temporary synthetic workspaces. Storage fault tests also cover partial writes, corrupt committed history, changed checksums, symlinks and preservation of unknown files.
