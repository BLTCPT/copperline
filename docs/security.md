# Security boundaries

This is a local synthetic-data prototype, not an enterprise multi-user service.

- Actor names and roles are caller-selected demo identities. Role checks and separation-of-duty rules run at the engine boundary, but there is no authentication, SSO, tenancy, authorization source or impersonation resistance.
- The default desktop/CLI performs no business network calls. `serve PORT` explicitly exposes newline JSON on 127.0.0.1 only, with a 2 MiB request cap and socket timeouts. Trusted local processes can send commands. There is no HTTP endpoint, CORS surface, TLS, token authentication or remote binding option.
- No credentials, SAP SDKs, company data, telemetry, payments or ERP writes are included. Future adapters must obtain scoped authorization and keep secrets outside snapshots/logs. The integration contract is an interface proposal, not compatibility.
- Data directory permissions rely on the host account. Use a private local directory, never a shared/network directory or untrusted downloaded workspace. Symlink/nonregular snapshots are rejected; pending snapshots use exclusive creation. Directory-level concurrent malicious tampering is outside the local trust boundary.
- SHA-256 detects accidental corruption, not adversarial forgery. No digital signatures, immutable external audit service, backups, retention policy or encryption-at-rest are provided.
- CSV imports reject invalid/duplicate IDs atomically. Export strings are quoted and formula-prefixed cells escaped to reduce spreadsheet formula injection. Export refuses existing paths and unapproved quotes.
- Corrupt committed snapshots fail closed. Preserve the entire directory and inspect before recovery; do not delete revisions to bypass validation.
- Locked workspace errors are expected if desktop and CLI open the same directory. Use the local API process for automation, or close the desktop first.

Production gates include authenticated immutable actor IDs, document ownership policy, organization-scoped permissions, audited migrations, threat model, load limits, storage durability/backup tests, signed builds, trusted update channel and independent security review.
