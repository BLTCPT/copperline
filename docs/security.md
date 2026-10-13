# Security boundaries

This is a local synthetic-data prototype, not an enterprise multi-user service.

- Actor names and roles are caller-selected demo identities. Role checks, owner-only quote/contract/conversion/correction/settlement actions and contributor-aware review run at the engine boundary, but there is no authentication, SSO, tenancy, authorization source or impersonation resistance.
- The default desktop/CLI performs no business network calls. `serve PORT` explicitly exposes newline JSON on 127.0.0.1 only, with a 2 MiB request cap, 5-second inactivity timeouts and at most 16 clients. Individual requests serialize through the engine; separate clients can remain connected. Trusted local processes can send commands. There is no HTTP endpoint, CORS surface, TLS, token authentication or remote binding option.
- No credentials, SAP SDKs, company data, telemetry, payments or ERP writes are included. Local settlement approval records a signed reconciliation adjustment only; it does not mark cash as paid or post a liability externally. Future adapters must obtain scoped authorization and keep secrets outside snapshots/logs. The integration contract is an interface proposal, not compatibility.
- Data directory permissions rely on the host account. Use a private local directory, never a shared/network directory or untrusted downloaded workspace. Symlink/nonregular snapshots are rejected; pending snapshots use exclusive creation. Directory-level concurrent malicious tampering is outside the local trust boundary.
- SHA-256 detects accidental corruption, not adversarial forgery. Portable exact-history backup and guarded restore into a new directory are provided locally; no digital signatures, immutable external audit service, retention policy or encryption-at-rest is provided. Archives contain the complete local business history and must be handled accordingly. See backup.md for caps, failure handling and durability limits.
- CSV imports reject invalid/duplicate IDs atomically. Export strings are quoted and formula-prefixed cells escaped to reduce spreadsheet formula injection. Export refuses existing paths and unapproved quotes.
- Corrupt committed snapshots fail closed. Preserve the entire directory; storage-inspect and checksum-verified quarantine-pending can preserve the exactly-next pending file without promoting it. Committed corruption prevents recovery. Do not delete revisions to bypass validation.
- Locked workspace errors are expected if desktop and CLI open the same directory. Use the local API process for automation, or close the desktop first.

Production gates include authenticated immutable actor IDs, document ownership policy, organization-scoped permissions, audited migrations, threat model, load limits, storage durability/backup tests, signed builds, trusted update channel and independent security review.

Conversion books are user-authored synthetic inputs; no market data provider is queried. Review does not establish rate accuracy or authenticated approval. Reporting snapshots record arithmetic and source evidence; they do not execute foreign exchange or post accounting entries.

Quote booking is a local record derived from an approved synthetic quote, not proof of customer acceptance, delivery, external invoicing or payment. Booking provenance and rate knowledge cutoffs detect inconsistent local history; they do not authenticate actors or source rate accuracy. Existing role, file trust and corruption boundaries apply.

Quantity credits and nominal invoice-period closes are local synthetic controls. Non-contributor Finance review uses caller-selected identities. A nominal close freezes invoice IDs/amounts only; it is not an authenticated accounting-period lock, legal credit note or GAAP/IFRS assertion. No reopen, carry-forward posting, tax treatment or inventory valuation is provided.
