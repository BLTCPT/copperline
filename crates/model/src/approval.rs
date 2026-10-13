//! Frozen quote review routes. Caller-selected demo identities are not authentication.
use super::*;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovalThresholds {
    pub max_discount: Bps,
    pub min_margin: Bps,
    pub usd_value: Money,
    pub eur_value: Money,
    pub gbp_value: Money,
}
impl Default for ApprovalThresholds {
    fn default() -> Self {
        Self {
            max_discount: Bps(1500),
            min_margin: Bps(2500),
            usd_value: Money(10_000_000),
            eur_value: Money(10_000_000),
            gbp_value: Money(10_000_000),
        }
    }
}
impl ApprovalThresholds {
    pub fn validate(&self) -> Result<()> {
        self.max_discount.validate()?;
        self.min_margin.validate()?;
        for amount in [self.usd_value, self.eur_value, self.gbp_value] {
            amount.nonnegative()?;
            ensure(amount.0 > 0, "Approval value thresholds must be positive")?;
        }
        Ok(())
    }
    pub fn value_for(&self, currency: Currency) -> Money {
        match currency {
            Currency::USD => self.usd_value,
            Currency::EUR => self.eur_value,
            Currency::GBP => self.gbp_value,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuoteApprovalPolicy {
    pub version: u64,
    pub revision: u64,
    pub thresholds: ApprovalThresholds,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalStage {
    pub role: Role,
    pub reason: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuoteReview {
    pub actor: Actor,
    pub note: String,
    pub revision: u64,
    pub approved: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuoteApprovalCycle {
    pub submitted_revision: u64,
    pub submitted_version: u64,
    pub source_revision: u64,
    pub submitted_by: String,
    pub policy: Option<QuoteApprovalPolicy>,
    pub pricing_policy: Policy,
    pub lines: Vec<Evaluation>,
    pub stages: Vec<ApprovalStage>,
    pub reviews: Vec<QuoteReview>,
}
impl QuoteApprovalCycle {
    pub fn status(&self) -> Status {
        if self.reviews.last().is_some_and(|r| !r.approved) {
            Status::Rejected
        } else if self.reviews.len() == self.stages.len() {
            Status::Approved
        } else {
            Status::Pending
        }
    }
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuoteGovernance {
    pub contributors: BTreeSet<String>,
    pub cycles: Vec<QuoteApprovalCycle>,
}

/// Discount is strictly above, margin strictly below, value at or above its threshold.
/// Margin compares exact totals, never the rounded display basis points.
pub fn quote_approval_stages(
    lines: &[Evaluation],
    policy: Option<&QuoteApprovalPolicy>,
) -> Result<Vec<ApprovalStage>> {
    let first = lines
        .first()
        .ok_or_else(|| error("Quote requires at least one line"))?;
    let mut stages = vec![ApprovalStage {
        role: Role::Approver,
        reason: "Independent commercial review".into(),
    }];
    if let Some(policy) = policy {
        policy.thresholds.validate()?;
        let mut pocket = Money(0);
        let mut margin = Money(0);
        for line in lines {
            ensure(
                line.currency == first.currency,
                "Mixed currency approval route",
            )?;
            pocket = pocket.add(line.pocket_total)?;
            margin = margin.add(line.margin_total)?;
        }
        let t = &policy.thresholds;
        let mut reasons = vec![];
        if lines.iter().any(|line| {
            (i128::from(line.list_unit.0) - i128::from(line.invoice_unit.0)) * 10_000
                > i128::from(line.list_unit.0) * i128::from(t.max_discount.0)
        }) {
            reasons.push("effective invoice discount above threshold");
        }
        if i128::from(margin.0) * 10_000 < i128::from(pocket.0) * i128::from(t.min_margin.0) {
            reasons.push("weighted margin below threshold");
        }
        if pocket >= t.value_for(first.currency) {
            reasons.push("pocket value at or above threshold");
        }
        if !reasons.is_empty() {
            stages.push(ApprovalStage {
                role: Role::Finance,
                reason: reasons.join("; "),
            });
        }
    }
    Ok(stages)
}

pub fn quote_target(id: &str) -> String {
    format!("quote:{id}")
}
fn author_command(command: &str) -> bool {
    matches!(
        command,
        "quote_create"
            | "quote_add_line"
            | "quote_replace_line"
            | "quote_remove_line"
            | "quote_reprice"
            | "quote_revise"
    )
}
fn legacy_author(a: &Audit, id: &str) -> bool {
    match a.command.as_str() {
        "quote_create" => a.detail == format!("Created quote {id}"),
        "quote_add_line" => a.detail == format!("Added line and recalculated {id}"),
        "quote_remove_line" => a.detail == format!("Removed line from {id}"),
        "quote_reprice" => a
            .detail
            .starts_with(&format!("Repriced {id} against revision ")),
        "quote_revise" => a.detail == format!("Reopened {id}; previous approval no longer applies"),
        _ => false,
    }
}
impl State {
    pub fn quote_contributors(&self, q: &Quote) -> BTreeSet<String> {
        let target = quote_target(&q.id);
        let mut result = BTreeSet::from([q.owner.clone()]);
        for a in &self.audit {
            if (a.target.as_deref() == Some(&target) && author_command(&a.command))
                || (a.target.is_none() && legacy_author(a, &q.id))
            {
                result.insert(a.actor.name.clone());
            }
        }
        result
    }
    pub(crate) fn validate_quote_governance(&self) -> Result<()> {
        ensure(
            self.audit
                .iter()
                .filter(|a| a.command == "quote_approval_policy_set")
                .map(|a| a.revision)
                .collect::<Vec<_>>()
                == self
                    .quote_approval_policies
                    .iter()
                    .map(|p| p.revision)
                    .collect::<Vec<_>>(),
            "Approval policy history does not match audit",
        )?;
        let mut prior_revision = 0;
        for (i, policy) in self.quote_approval_policies.iter().enumerate() {
            policy.thresholds.validate()?;
            ensure(
                policy.version == i as u64 + 1
                    && policy.revision > prior_revision
                    && policy.revision <= self.revision,
                "Approval policy version/revision mismatch",
            )?;
            prior_revision = policy.revision;
            let a = self
                .audit
                .iter()
                .find(|a| a.revision == policy.revision)
                .ok_or_else(|| error("Approval policy audit missing"))?;
            ensure(
                a.actor.role == Role::Admin && a.command == "quote_approval_policy_set",
                "Approval policy audit mismatch",
            )?;
        }
        for q in self.quotes.values() {
            let target = quote_target(&q.id);
            let creators: Vec<_> = self
                .audit
                .iter()
                .filter(|a| {
                    a.command == "quote_create"
                        && (a.target.as_deref() == Some(&target)
                            || (a.target.is_none() && legacy_author(a, &q.id)))
                })
                .collect();
            ensure(
                creators.len() == 1 && creators.first().is_some_and(|a| a.actor.name == q.owner),
                "Quote owner must match its immutable creation audit",
            )?;
            let has_new_audit = self
                .audit
                .iter()
                .any(|a| a.target.as_deref() == Some(&target));
            let Some(governance) = &q.governance else {
                ensure(
                    !has_new_audit,
                    "Governed quote cannot discard approval provenance",
                )?;
                continue;
            };
            ensure(
                governance.contributors == self.quote_contributors(q),
                "Quote contributor provenance mismatch",
            )?;
            ensure(
                governance.cycles.len() <= 100,
                "Quote approval cycle limit reached; create a new quote",
            )?;
            let submissions: Vec<u64> = self
                .audit
                .iter()
                .filter(|a| a.command == "quote_submit" && a.target.as_deref() == Some(&target))
                .map(|a| a.revision)
                .collect();
            ensure(
                submissions
                    == governance
                        .cycles
                        .iter()
                        .map(|cycle| cycle.submitted_revision)
                        .collect::<Vec<_>>(),
                "Quote cannot discard or invent submission cycles",
            )?;
            if let Some(first) = submissions.first() {
                let reviews: Vec<u64> = self
                    .audit
                    .iter()
                    .filter(|a| {
                        a.command == "quote_decide"
                            && a.target.as_deref() == Some(&target)
                            && a.revision > *first
                    })
                    .map(|a| a.revision)
                    .collect();
                ensure(
                    reviews
                        == governance
                            .cycles
                            .iter()
                            .flat_map(|cycle| cycle.reviews.iter().map(|r| r.revision))
                            .collect::<Vec<_>>(),
                    "Quote cannot discard or invent route reviews",
                )?;
            }
            if q.status == Status::Draft {
                ensure(
                    governance
                        .cycles
                        .last()
                        .is_none_or(|cycle| cycle.status() != Status::Pending),
                    "Pending approval must finish before quote revision",
                )?;
            }
            let mut previous_terminal = 0;
            for (index, cycle) in governance.cycles.iter().enumerate() {
                ensure(
                    cycle.submitted_revision > previous_terminal
                        && cycle.submitted_revision <= self.revision
                        && cycle.source_revision < cycle.submitted_revision
                        && cycle.submitted_version > 1,
                    "Invalid quote submission chronology",
                )?;
                let audit = self
                    .audit
                    .iter()
                    .find(|a| a.revision == cycle.submitted_revision)
                    .ok_or_else(|| error("Quote submission audit missing"))?;
                ensure(
                    audit.command == "quote_submit"
                        && audit.target.as_deref() == Some(&target)
                        && audit.actor.name == cycle.submitted_by
                        && cycle.submitted_by == q.owner,
                    "Quote submission target/owner mismatch",
                )?;
                if let Some(policy) = &cycle.policy {
                    ensure(
                        policy.revision < cycle.submitted_revision
                            && self.quote_approval_policies.iter().any(|p| p == policy),
                        "Frozen approval policy not found",
                    )?;
                }
                let policy_at_submit = self
                    .quote_approval_policies
                    .iter()
                    .rfind(|p| p.revision < cycle.submitted_revision);
                ensure(
                    cycle.policy.as_ref() == policy_at_submit,
                    "Submission did not freeze the active approval policy",
                )?;
                cycle.pricing_policy.validate()?;
                ensure(
                    !cycle.lines.is_empty() && cycle.lines.len() <= 200,
                    "Invalid frozen submission lines",
                )?;
                for line in &cycle.lines {
                    line.validate_snapshot()?;
                    line.check_policy(&cycle.pricing_policy)?;
                    self.validate_snapshot_references(line, cycle.source_revision)?;
                    ensure(
                        !line.hard_block
                            && line.request.customer_id == q.customer_id
                            && line.request.date == q.date,
                        "Frozen submission violates quote context/guardrail",
                    )?;
                }
                ensure(
                    cycle.stages == quote_approval_stages(&cycle.lines, cycle.policy.as_ref())?
                        && cycle.reviews.len() <= cycle.stages.len(),
                    "Frozen approval route mismatch",
                )?;
                let mut previous_review = cycle.submitted_revision;
                let mut actors = BTreeSet::new();
                for (step, review) in cycle.reviews.iter().enumerate() {
                    text(&review.actor.name)?;
                    text(&review.note)?;
                    ensure(
                        review.revision > previous_review && review.revision <= self.revision,
                        "Review must follow submission and preceding review",
                    )?;
                    previous_review = review.revision;
                    ensure(
                        review.actor.role == cycle.stages[step].role
                            && review.actor.name != cycle.submitted_by
                            && !governance.contributors.contains(&review.actor.name)
                            && actors.insert(&review.actor.name),
                        "Review role or separation of duties violated",
                    )?;
                    ensure(
                        review.approved || step + 1 == cycle.reviews.len(),
                        "Rejected approval cycle cannot continue",
                    )?;
                    let a = self
                        .audit
                        .iter()
                        .find(|a| a.revision == review.revision)
                        .ok_or_else(|| error("Quote review audit missing"))?;
                    ensure(
                        a.command == "quote_decide"
                            && a.target.as_deref() == Some(&target)
                            && a.actor == review.actor,
                        "Quote review target/actor audit mismatch",
                    )?;
                    let decided_status = if !review.approved {
                        Status::Rejected
                    } else if step + 1 == cycle.stages.len() {
                        Status::Approved
                    } else {
                        Status::Pending
                    };
                    let decided_version = cycle
                        .submitted_version
                        .checked_add(step as u64 + 1)
                        .ok_or_else(|| error("Approval version overflow"))?;
                    ensure(
                        a.detail
                            == format!(
                                "Quote {}: {:?}; document v{}",
                                q.id, decided_status, decided_version
                            ),
                        "Quote review outcome/version audit mismatch",
                    )?;
                }
                let status = cycle.status();
                if status != Status::Pending {
                    let review = cycle
                        .reviews
                        .last()
                        .ok_or_else(|| error("Terminal route requires a review"))?;
                    ensure(
                        q.decisions.iter().any(|d| {
                            d.actor == review.actor.name
                                && d.note == review.note
                                && d.revision == review.revision
                                && d.approved == review.approved
                        }),
                        "Terminal quote decision missing",
                    )?;
                }
                if index + 1 < governance.cycles.len() {
                    ensure(
                        status != Status::Pending,
                        "Old approval cycle cannot remain pending",
                    )?;
                }
                previous_terminal = previous_review;
                if index + 1 == governance.cycles.len() && q.status != Status::Draft {
                    let expected_version = cycle
                        .submitted_version
                        .checked_add(cycle.reviews.len() as u64)
                        .ok_or_else(|| error("Approval version overflow"))?;
                    ensure(
                        q.status == status
                            && q.submitted_by.as_deref() == Some(&cycle.submitted_by)
                            && q.version == expected_version
                            && q.lines == cycle.lines
                            && q.policy == cycle.pricing_policy
                            && q.source_revision == cycle.source_revision,
                        "Current quote does not match its frozen approval cycle",
                    )?;
                    if status != Status::Pending {
                        let review = cycle
                            .reviews
                            .last()
                            .ok_or_else(|| error("Missing terminal review"))?;
                        ensure(
                            q.decisions
                                .last()
                                .is_some_and(|d| d.revision == review.revision),
                            "Current decision does not finish the current route",
                        )?;
                    }
                }
            }
            for d in &q.decisions {
                if let Some(a) = self
                    .audit
                    .iter()
                    .find(|a| a.revision == d.revision && a.target.is_some())
                {
                    ensure(
                        a.target.as_deref() == Some(&target)
                            && !governance.contributors.contains(&d.actor),
                        "Decision belongs to another quote or a contributor",
                    )?;
                }
            }
        }
        Ok(())
    }
}
