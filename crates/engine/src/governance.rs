use super::*;

fn quote_id(c: &Command) -> Option<&str> {
    match c {
        Command::QuoteCreate { id, .. }
        | Command::QuoteAddLine { id, .. }
        | Command::QuoteReplaceLines { id, .. }
        | Command::QuoteReplaceLine { id, .. }
        | Command::QuoteRemoveLine { id, .. }
        | Command::QuoteReprice { id, .. }
        | Command::QuoteSubmit { id, .. }
        | Command::QuoteDecide { id, .. }
        | Command::QuoteRevise { id, .. } => Some(id),
        _ => None,
    }
}
pub(super) fn target(c: &Command) -> Option<String> {
    if let Some(id) = quote_id(c) {
        return Some(quote_target(id));
    }
    match c {
        Command::ConversionCreate { terms } => Some(conversion_target(&terms.id)),
        Command::ConversionImport { id, .. }
        | Command::ConversionSubmit { id, .. }
        | Command::ConversionDecide { id, .. }
        | Command::ConversionWithdraw { id, .. } => Some(conversion_target(id)),
        Command::ReportConvert { id, .. } => Some(report_target(id)),
        Command::ContractCreate { terms } => Some(contract_target(&terms.id, terms.edition)),
        Command::ContractSubmit { id, edition, .. }
        | Command::ContractDecide { id, edition, .. }
        | Command::ContractWithdraw { id, edition, .. } => Some(contract_target(id, *edition)),
        Command::RebateAccrue { id, .. } => Some(format!("accrual:{id}")),
        Command::TransactionAdd(t) => Some(format!("invoice:{}", t.id)),
        Command::CorrectionCreate { proposal } => Some(format!("correction:{}", proposal.id)),
        Command::CorrectionSubmit { id, .. }
        | Command::CorrectionDecide { id, .. }
        | Command::CorrectionWithdraw { id, .. } => Some(format!("correction:{id}")),
        Command::SettlementCreate { id, .. }
        | Command::SettlementSubmit { id, .. }
        | Command::SettlementDecide { id, .. }
        | Command::SettlementWithdraw { id, .. } => Some(format!("settlement:{id}")),
        Command::ListCalculate { id, .. }
        | Command::ListSubmit { id, .. }
        | Command::ListDecide { id, .. }
        | Command::ListPublish { id, .. } => Some(format!("list:{id}")),
        Command::GridCalculate { id, .. }
        | Command::GridSubmit { id, .. }
        | Command::GridDecide { id, .. } => Some(format!("grid:{id}")),
        Command::AgreementCreate { terms } => Some(format!("agreement:{}", terms.id)),
        Command::AgreementSubmit { id, .. } | Command::AgreementDecide { id, .. } => {
            Some(format!("agreement:{id}"))
        }
        _ => None,
    }
}

pub(super) fn prepare(s: &mut State, actor: &Actor, c: &Command) -> Result<()> {
    let owner = match c {
        Command::ListSubmit { id, .. } | Command::ListPublish { id, .. } => {
            s.price_lists.get(id).map(|d| &d.owner)
        }
        Command::GridSubmit { id, .. } => s.grid.get(id).map(|d| &d.owner),
        Command::AgreementSubmit { id, .. } => s.agreements.get(id).map(|d| &d.owner),
        _ => None,
    };
    if let Some(owner) = owner {
        ensure(
            owner == &actor.name,
            "Only the document owner may submit or publish",
        )?;
    }
    let Some(id) = quote_id(c) else {
        return Ok(());
    };
    if matches!(c, Command::QuoteCreate { .. }) {
        return Ok(());
    }
    let q = s.quotes.get(id).ok_or_else(|| error("Quote not found"))?;
    if !matches!(c, Command::QuoteDecide { .. }) {
        ensure(
            q.owner == actor.name,
            "Only the quote owner may edit, revise or submit; Admin does not bypass ownership",
        )?;
    }
    let contributors = s.quote_contributors(q);
    let q = quote_mut(s, id)?;
    let governance = q.governance.get_or_insert_with(|| QuoteGovernance {
        contributors: contributors.clone(),
        cycles: vec![],
    });
    governance.contributors = contributors;
    if matches!(
        c,
        Command::QuoteAddLine { .. }
            | Command::QuoteReplaceLine { .. }
            | Command::QuoteReplaceLines { .. }
            | Command::QuoteRemoveLine { .. }
            | Command::QuoteReprice { .. }
            | Command::QuoteRevise { .. }
    ) {
        governance.contributors.insert(actor.name.clone());
    }
    Ok(())
}

pub(super) fn apply(s: &mut State, actor: &Actor, c: &Command) -> Result<String> {
    match c {
        Command::QuoteApprovalPolicySet {
            expected_version,
            thresholds,
        } => {
            thresholds.validate()?;
            let latest = s
                .quote_approval_policies
                .last()
                .map(|p| p.version)
                .unwrap_or(0);
            ensure(
                latest == *expected_version,
                "Approval policy version changed",
            )?;
            let version = next_version(latest)?;
            s.quote_approval_policies.push(QuoteApprovalPolicy {
                version,
                revision: next_version(s.revision)?,
                thresholds: thresholds.clone(),
            });
            Ok(format!(
                "Set quote approval policy v{version}; pending routes remain frozen"
            ))
        }
        Command::QuoteSubmit { id, version } => {
            let policy = s.quote_approval_policies.last().cloned();
            let revision = next_version(s.revision)?;
            let q = quote_mut(s, id)?;
            draft(q, *version)?;
            totals(&q.lines)?;
            ensure(
                !q.lines.iter().any(|l| l.hard_block),
                "Quote violates a hard guardrail; revise the price",
            )?;
            let governance = q
                .governance
                .as_mut()
                .ok_or_else(|| error("Quote governance missing"))?;
            ensure(
                governance.cycles.len() < 100,
                "Quote approval cycle limit reached; create a new quote",
            )?;
            let stages = quote_approval_stages(&q.lines, policy.as_ref())?;
            let count = stages.len();
            q.version = next_version(q.version)?;
            governance.cycles.push(QuoteApprovalCycle {
                submitted_revision: revision,
                submitted_version: q.version,
                source_revision: q.source_revision,
                submitted_by: actor.name.clone(),
                policy,
                pricing_policy: q.policy.clone(),
                lines: q.lines.clone(),
                stages,
                reviews: vec![],
            });
            q.status = Status::Pending;
            q.submitted_by = Some(actor.name.clone());
            Ok(format!(
                "Submitted {id} for {count} sequential approval stage(s)"
            ))
        }
        Command::QuoteDecide {
            id,
            version,
            approve,
            note,
        } => {
            text(note)?;
            let revision = next_version(s.revision)?;
            let q = quote_mut(s, id)?;
            ensure(
                q.version == *version && q.status == Status::Pending,
                "Decision requires the current pending quote version",
            )?;
            let governance = q
                .governance
                .as_mut()
                .ok_or_else(|| error("Quote governance missing"))?;
            ensure(
                q.owner != actor.name
                    && q.submitted_by.as_ref() != Some(&actor.name)
                    && !governance.contributors.contains(&actor.name),
                "Quote owner, submitter and every contributor are excluded from approval",
            )?;
            let mut status = if *approve {
                Status::Approved
            } else {
                Status::Rejected
            };
            if let Some(cycle) = governance.cycles.last_mut() {
                ensure(
                    cycle.status() == Status::Pending,
                    "Current approval cycle is already complete",
                )?;
                let stage = cycle
                    .stages
                    .get(cycle.reviews.len())
                    .ok_or_else(|| error("No pending approval stage"))?;
                ensure(
                    actor.role == stage.role,
                    "Actor role does not match the next approval stage; Admin cannot substitute",
                )?;
                ensure(
                    !cycle.reviews.iter().any(|r| r.actor.name == actor.name),
                    "Each approval stage requires a different person",
                )?;
                cycle.reviews.push(QuoteReview {
                    actor: actor.clone(),
                    note: note.clone(),
                    revision,
                    approved: *approve,
                });
                status = cycle.status();
            } else {
                // M1/M2 pending documents retain their original one-step route, but
                // contributor exclusions still apply. No retroactive policy escalation.
                ensure(
                    matches!(actor.role, Role::Approver | Role::Admin),
                    "Legacy pending quote requires an approver",
                )?;
            }
            q.status = status;
            if status != Status::Pending {
                q.decisions.push(Decision {
                    actor: actor.name.clone(),
                    note: note.clone(),
                    revision,
                    approved: *approve,
                });
            }
            q.version = next_version(q.version)?;
            Ok(format!(
                "Quote {id}: {:?}; document v{}",
                q.status, q.version
            ))
        }
        _ => Err(error("Not a quote governance command")),
    }
}
