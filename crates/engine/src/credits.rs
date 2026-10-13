use super::*;

pub(super) fn apply(s: &mut State, actor: &Actor, command: &Command) -> Result<String> {
    match command {
        Command::CreditCreate { proposal } => {
            ensure(
                matches!(actor.role, Role::Analyst | Role::Admin),
                "Only an analyst or admin may propose a credit",
            )?;
            let record = s.credit_preview(proposal, &actor.name)?;
            let detail = credit_created_detail(&record)?;
            s.credits.insert(proposal.id.clone(), record);
            Ok(detail)
        }
        Command::CreditSubmit { id, version } => {
            let record = s.credits.get(id).ok_or_else(|| error("Credit not found"))?;
            owner(record, actor)?;
            ensure(
                record.version == *version && record.status == RecordStatus::Draft,
                "Submit the current draft credit version",
            )?;
            s.ensure_credit_current(record, s.revision)?;
            let revision = next_version(s.revision)?;
            let record = s
                .credits
                .get_mut(id)
                .ok_or_else(|| error("Credit not found"))?;
            record.status = RecordStatus::Pending;
            record.version = next_version(record.version)?;
            record.submitted_revision = Some(revision);
            Ok(format!("Submitted credit {id}"))
        }
        Command::CreditDecide {
            id,
            version,
            approve,
            note,
        } => {
            text(note)?;
            ensure(
                actor.role == Role::Finance,
                "Credit decisions require Finance; Admin cannot substitute",
            )?;
            let record = s.credits.get(id).ok_or_else(|| error("Credit not found"))?;
            ensure(
                record.version == *version && record.status == RecordStatus::Pending,
                "Decide the current pending credit version",
            )?;
            ensure(
                !record.contributors.contains(&actor.name),
                "Credit contributors cannot review their own evidence",
            )?;
            if *approve {
                s.ensure_credit_current(record, s.revision)?;
            }
            let review = RecordReview {
                actor: actor.clone(),
                approved: *approve,
                note: note.clone(),
                revision: next_version(s.revision)?,
            };
            let record = s
                .credits
                .get_mut(id)
                .ok_or_else(|| error("Credit not found"))?;
            let detail = credit_decision_detail(record, &review)?;
            record.status = if *approve {
                RecordStatus::Approved
            } else {
                RecordStatus::Rejected
            };
            record.version = next_version(record.version)?;
            record.review = Some(review);
            super::commercial::analytics(s)?;
            Ok(detail)
        }
        Command::CreditWithdraw { id, version } => {
            let revision = next_version(s.revision)?;
            let record = s
                .credits
                .get_mut(id)
                .ok_or_else(|| error("Credit not found"))?;
            owner(record, actor)?;
            ensure(
                record.version == *version
                    && matches!(record.status, RecordStatus::Draft | RecordStatus::Pending),
                "Withdraw the current draft or pending credit version",
            )?;
            record.status = RecordStatus::Withdrawn;
            record.version = next_version(record.version)?;
            record.withdrawn_revision = Some(revision);
            Ok(format!("Withdrew credit {id}"))
        }
        _ => Err(error("Not a credit command")),
    }
}

fn owner(record: &Credit, actor: &Actor) -> Result<()> {
    ensure(
        matches!(actor.role, Role::Analyst | Role::Admin) && actor.name == record.owner,
        "Only the credit owner may submit or withdraw; Admin does not bypass ownership",
    )
}
