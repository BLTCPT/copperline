# Status — 2026-10-13 morning stabilization

Copperline is ready for a controlled synthetic demo. It remains an early local prototype, not production software or complete Pricefx parity. M9 financial behavior is unchanged; morning work corrects native review/reporting usability, clarifies capabilities and prerequisites, improves license provenance, and packages a prepared full-workflow Windows demo.

Exact source `57032bf9c59a2189e2ae6bcc85ce79877d229253` passes [three-OS CI](https://github.com/BLTCPT/copperline/actions/runs/38395423236): 259 Rust checks, notice verification, all eight process workflows, native release builds and workload/archive drill. Windows packaging validates/restores a fresh revision-65 demo archive. The public fresh-checkout build/seed and a separate r65→r67 morning drill pass; nine exports and all snapshots preserve through restore. Six macOS native screenshots are inspected. See [verification](../evidence/verification.md) and the [ten-minute runbook](morning-demo.md) for exact artifact hash, expiry, sign-in requirement and limitations.

Independent M9 review found no new high-confidence P1/P2 blocker. The source/history audit found no credential-pattern candidate or private company data within its documented scope. Recovered dependency/font notices cover 313 locked records; one dispatch attribution gap and Apple-binding upstream legal caveats remain explicit. This is not blanket legal clearance. See [repository review](repository-review.md).

No authenticated enterprise identity, tenancy, external invoice/payment/ERP write, tax/compliance, optimization/AI, promotions or enterprise-scale service exists. Nominal close excludes FX/rebate calculations. Windows interactive UI and power-loss durability are unverified; builds/archives are unsigned and archives unencrypted. Full-state snapshot limits remain. [Gaps](gaps.md) and [roadmap](roadmap.md) identify the next work and decisions.

M9 financial source `a36d4877228b14732aff84d77d32776e9c8260ae`, prior Windows artifacts and compatibility evidence remain in [M9 verification](../evidence/m9-verification.md). Historical M7 workload measurements are not a current enterprise performance claim.

Reference guidance read: GridCraft AGENTS.md, docs/architecture.md, docs/release-playbook.md and docs/releasing.md at the NOTICE reference commit. The referenced craftrules checkout was absent; three public raw URLs returned 404. No relevant project `.agents/skills` directory was found. The available frontend-design skill informed the original UI. No GridCraft source or artwork was copied.
