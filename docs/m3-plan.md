# M3 scope and acceptance

This milestone deepens three partial capabilities in [the parity matrix](parity.md). It does not claim full platform parity.

| Existing gap | M3 deliverable | Acceptance |
|---|---|---|
| One-step approvals and missing quote ownership enforcement | Owner-only quote authoring, persistent contributor exclusions, versioned conditional commercial → Finance route | Engine rejects editing Admin bypass, own-work approval, skipped/repeated stages and wrong-role decisions; exact threshold tests, frozen policy, retry/reopen and corruption regressions |
| Approved grids/lists do not feed quotes | Explicit approved source selection with pinned evidence | No arbitrary lookup or fallback; point-date/customer/product/currency/unit/quantity eligibility; historical quote/export remains unchanged after source replacement |
| Limited concurrency and interrupted-write recovery | Bounded concurrent local clients plus inspect/quarantine commands | Concurrent revisions/retries remain atomic, one idle client does not monopolize the server, unknown pending bytes are never silently deleted or promoted |

Native views must expose the pricing basis, active approval policy, required stages and current progress. The native app, CLI and API must share the engine checks. Existing M1/M2 snapshots and request fingerprints must remain readable/replayable where their original behavior is safe; new optional fields omit empty/default serialization.

Quote approval policy uses exact comparisons: effective invoice discount strictly above the configured percentage, weighted pocket margin strictly below the configured percentage, or pocket revenue at/above a currency-specific amount triggers Finance after commercial review. Stages require distinct actors, and neither Admin nor a contributor can substitute for the required reviewer. Existing pending legacy quotes retain their original one-step route, with contributor exclusions applied on the next decision. Lists, grids and agreements retain their existing single review step.

Identity remains explicitly caller-selected local demo identity. No external identity provider, credential, tenancy, production service or real business integration is introduced. Pricing sources remain point-date snapshots; FX, UOM conversion and open-ended contract validity are deferred until their semantics can be implemented and verified coherently. Existing accruals remain alternative full-period snapshots, never additive payables.

Delivery gates: independent governance/pricing/persistence review, layer and attribution gates, fmt, strict Clippy, unit/integration/property tests, native UI interaction tests, process/transport/recovery E2E, then Windows/macOS/Linux CI for the exact source commit. Update the existing Library handoff identity and provide confirmed screenshot IDs and a public CI artifact reference/hash.
