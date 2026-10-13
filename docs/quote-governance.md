# Quote ownership and sequential approval

M3 fixes a confirmed M2 defect: an administrator could edit somebody else's quote and later approve those edits after another person submitted it. Quote creation now establishes an immutable owner. Only that same actor name, with Sales or Admin role, may add/remove lines, reprice, revise or submit the quote. Admin has no ownership bypass. There is no owner reassignment or authoring override command.

Every historical substantive quote contributor is excluded from all review stages. The set is retained across revisions because retained lines can still contain earlier contributions. Opening a legacy workspace does not rewrite it. On a quote's next mutation, its contributor set is reconstructed from original quote audit records, including earlier Admin editors. A legacy pending quote keeps its original one-step approval semantics, but an earlier editor cannot newly approve it. Completed legacy decisions are not retroactively rewritten.

New quote commands write an explicit quote target into audit records. New submissions freeze a review cycle containing the submission revision/version, pricing revision, calculated lines, pricing guardrails, approval policy and ordered stages. Persisted-state validation ties reviews to the exact quote, actor, role, submission, stage and audit revision. Removing cycles, changing owners or copying another quote's review is rejected. New empty/optional fields are omitted so old snapshot checksums and request fingerprints remain compatible.

## Policy configuration

Without a configured policy, newly submitted quotes need one `approver` review. An Admin may save a versioned policy through `quote_approval_policy_set`:

```json
{
  "command": "quote_approval_policy_set",
  "params": {
    "expected_version": 0,
    "thresholds": {
      "max_discount": 1500,
      "min_margin": 2500,
      "usd_value": 10000000,
      "eur_value": 10000000,
      "gbp_value": 10000000
    }
  }
}
```

Use the normal mutation envelope with request ID, expected workspace revision and actor. Threshold percentages are basis points, values integer cents. The example amounts are independently USD/EUR/GBP 100,000; they are not FX conversions.

Commercial review is always first. Finance review is appended when any of these exact conditions holds:

- Any line's effective catalog-to-invoice discount is **strictly above** `max_discount`, including the selected rule or approved source price.
- The quote's weighted pocket margin is **strictly below** `min_margin`.
- Pocket revenue is **at or above** that quote currency's configured value threshold.

Comparisons use exact integer cross-products and totals, never rounded displayed margin percentages. Value thresholds must be positive. Hard pricing violations still block submission entirely.

Each stage uses `quote_decide` with the current quote version. The first requires role `approver`; the second requires role `finance`. Admin cannot substitute for either role on a new governed route. Contributors, owner, submitter and any earlier reviewer in that cycle are excluded. An intermediate approval increments the document version but leaves it Pending and unexportable. A rejection ends the cycle immediately. Only completion of every stage produces an Approved quote. A revision must be resubmitted and reviewed again.

Policy changes affect future submissions only. A pending or approved route retains its captured policy, reasons, lines and source. Exact retries return the original receipt without advancing a stage or creating another audit entry, even after subsequent decisions. Changed-content reuse of a request ID and stale document/workspace versions fail atomically.

## Native workbench

Use **Approval policy** as Casey / Admin to configure thresholds. Submit as the quote owner. **Approvals** and **Quotes** show the frozen route, required roles, reasons and progress. Review first as Riley / Approval and, when required, Jordan / Finance. The UI uses the same engine commands and receives the same errors as CLI/API clients.

Actor names and roles remain caller-selected **local demo identities**. These invariants demonstrate workflow authorization, not authentication, impersonation resistance, SSO, tenant isolation or real organizational access control. Multi-level delegated hierarchies and externally verified identity remain future work. Lists, grids and rebate agreements retain a single independent review; only their owner may submit, and only a list's owner may publish it locally.
