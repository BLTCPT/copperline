# M5 bounded contract-pricing slice

Add immutable customer price-contract editions around fixed prices or frozen approved grid/list origins. Existing point-date source semantics and serialized historical snapshots stay intact. Every edition has a half-open date interval, stable row IDs, non-overlapping half-open quantity bands, a customer/currency identity and explicit priority. No currency/unit conversion is inferred; FX and UOM schedules remain outside this milestone.

Only approved editions participate. Select the highest approved edition that has started before checking expiry, product or quantity, so no older edition resurrects. Edition numbers increase, and approved starts cannot go backward; equal starts allow same-period corrections. A future edition does not activate early. Automatic customer-contract selection is explicit opt-in and ranks qualified families by priority. Approval rejects equal-priority overlapping effective windows/product/bands. Effective windows end at the next approved edition start even if the old contractual end was later.

Creation freezes terms, source origin, source authors and unit values. Owner submission/withdrawal and independent commercial review mirror existing domain controls. Source fingerprints and exact audit events bind immutable evidence; caller-selected demo roles remain unauthenticated. Frozen source snapshots validate against the approvals known at their capture revision.

`quote_replace_line` changes one indexed Draft line and recalculates all retained lines in one transaction. Any failed pricing, stale source, owner check or persistence write leaves memory and disk unchanged. Existing submissions and approved exports remain reproducible.

Acceptance: native/API/CLI creation→approval→selection→quote→replacement→approval/export→reopen; date/quantity boundaries, conflicting contracts, same-start editions, non-resurrection, frozen-origin replacement, old snapshot/receipt compatibility, exact arithmetic, rollback, independent review and three-platform release/process CI.
