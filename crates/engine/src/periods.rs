use super::*;

pub(super) fn apply(state: &mut State, actor: &Actor, command: &Command) -> Result<String> {
    let revision = next_version(state.revision)?;
    match command {
        Command::PeriodCloseCreate { terms } => {
            ensure(
                matches!(actor.role, Role::Analyst | Role::Admin),
                "Only an analyst or admin may propose a period close",
            )?;
            let close = state.period_close_preview(terms, &actor.name)?;
            let detail = period_close_created_detail(&close)?;
            state.period_closes.insert(terms.id.clone(), close);
            Ok(detail)
        }
        Command::PeriodCloseSubmit { id, version }
        | Command::PeriodCloseWithdraw { id, version } => {
            let close = state
                .period_closes
                .get(id)
                .ok_or_else(|| error("Period close not found"))?;
            ensure(close.version == *version, "Period close version changed")?;
            ensure(
                matches!(actor.role, Role::Analyst | Role::Admin) && close.owner == actor.name,
                "Only the period close owner may submit or withdraw; Admin does not bypass ownership",
            )?;
            let submit = matches!(command, Command::PeriodCloseSubmit { .. });
            if submit {
                ensure(
                    close.status == RecordStatus::Draft,
                    "Submit only a draft period close",
                )?;
                state.ensure_period_close_current(close, state.revision)?;
            } else {
                ensure(
                    matches!(close.status, RecordStatus::Draft | RecordStatus::Pending),
                    "Withdraw only a draft or pending period close",
                )?;
            }
            let close = state
                .period_closes
                .get_mut(id)
                .ok_or_else(|| error("Period close not found"))?;
            close.version = next_version(close.version)?;
            if submit {
                close.status = RecordStatus::Pending;
                close.submitted_revision = Some(revision);
            } else {
                close.status = RecordStatus::Withdrawn;
                close.withdrawn_revision = Some(revision);
            }
            Ok(format!(
                "{} period close {id}",
                if submit { "Submitted" } else { "Withdrew" }
            ))
        }
        Command::PeriodCloseDecide {
            id,
            version,
            approve,
            note,
        } => {
            text(note)?;
            let close = state
                .period_closes
                .get(id)
                .ok_or_else(|| error("Period close not found"))?;
            ensure(
                close.version == *version && close.status == RecordStatus::Pending,
                "Decide the current pending period close",
            )?;
            ensure(
                actor.role == Role::Finance && !close.contributors.contains(&actor.name),
                "Period close requires a Finance reviewer independent of its owner and source authors",
            )?;
            if *approve {
                state.ensure_period_close_current(close, state.revision)?;
            }
            let close = state
                .period_closes
                .get_mut(id)
                .ok_or_else(|| error("Period close not found"))?;
            close.version = next_version(close.version)?;
            close.status = if *approve {
                RecordStatus::Approved
            } else {
                RecordStatus::Rejected
            };
            close.review = Some(RecordReview {
                actor: actor.clone(),
                approved: *approve,
                note: note.clone(),
                revision,
            });
            period_close_decided_detail(close)
        }
        _ => Err(error("Not a period-close command")),
    }
}
