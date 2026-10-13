use super::*;

pub(super) fn apply(state: &mut State, actor: &Actor, command: &Command) -> Result<String> {
    let revision = next_version(state.revision)?;
    match command {
        Command::SettlementCreate {
            id: key,
            accrual_id,
            reason,
        } => {
            id(key)?;
            text(reason)?;
            ensure(
                !state.settlements.contains_key(key),
                "Settlement ID already exists",
            )?;
            let accrual = state
                .accruals
                .get(accrual_id)
                .ok_or_else(|| error("Accrual not found"))?;
            let (prior_settlement_ids, previously_recognized) =
                state.settlement_baseline(&accrual.agreement_id, state.revision)?;
            let record = Settlement {
                id: key.clone(),
                accrual_id: accrual_id.clone(),
                agreement_id: accrual.agreement_id.clone(),
                agreement_version: accrual.agreement_version,
                currency: accrual.currency,
                source_revision: state.revision,
                owner: actor.name.clone(),
                reason: reason.clone(),
                version: 1,
                status: RecordStatus::Draft,
                created_revision: revision,
                submitted_revision: None,
                review: None,
                withdrawn_revision: None,
                contributors: state.settlement_contributors(accrual, &actor.name)?,
                prior_settlement_ids,
                previously_recognized,
                target_rebate: accrual.rebate,
                adjustment: accrual.rebate.sub(previously_recognized)?,
            };
            ensure(
                record.adjustment != Money(0),
                "No settlement adjustment is due",
            )?;
            state.ensure_settlement_current(&record, state.revision)?;
            let detail = settlement_created_detail(&record)?;
            state.settlements.insert(key.clone(), record);
            Ok(detail)
        }
        Command::SettlementSubmit { id, version } | Command::SettlementWithdraw { id, version } => {
            let record = state
                .settlements
                .get(id)
                .ok_or_else(|| error("Settlement not found"))?;
            ensure(record.version == *version, "Settlement version changed")?;
            ensure(
                record.owner == actor.name,
                "Only the settlement owner may submit or withdraw",
            )?;
            let submit = matches!(command, Command::SettlementSubmit { .. });
            if submit {
                ensure(
                    record.status == RecordStatus::Draft,
                    "Submit only a draft settlement",
                )?;
                state.ensure_settlement_current(record, state.revision)?;
            } else {
                ensure(
                    matches!(record.status, RecordStatus::Draft | RecordStatus::Pending),
                    "Only draft or pending settlements may be withdrawn",
                )?;
            }
            let record = state
                .settlements
                .get_mut(id)
                .ok_or_else(|| error("Settlement not found"))?;
            record.version = next_version(record.version)?;
            if submit {
                record.status = RecordStatus::Pending;
                record.submitted_revision = Some(revision);
            } else {
                record.status = RecordStatus::Withdrawn;
                record.withdrawn_revision = Some(revision);
            }
            Ok(format!(
                "{} settlement {id}",
                if submit { "Submitted" } else { "Withdrew" }
            ))
        }
        Command::SettlementDecide {
            id,
            version,
            approve,
            note,
        } => {
            text(note)?;
            let record = state
                .settlements
                .get(id)
                .ok_or_else(|| error("Settlement not found"))?;
            ensure(
                record.version == *version && record.status == RecordStatus::Pending,
                "Decide the current pending settlement",
            )?;
            ensure(
                actor.role == Role::Finance && !record.contributors.contains(&actor.name),
                "Settlement requires a Finance reviewer who did not contribute to its source data",
            )?;
            if *approve {
                state.ensure_settlement_current(record, state.revision)?;
            }
            let record = state
                .settlements
                .get_mut(id)
                .ok_or_else(|| error("Settlement not found"))?;
            record.version = next_version(record.version)?;
            record.status = if *approve {
                RecordStatus::Approved
            } else {
                RecordStatus::Rejected
            };
            record.review = Some(RecordReview {
                actor: actor.clone(),
                approved: *approve,
                note: note.clone(),
                revision,
            });
            settlement_decided_detail(record, *approve)
        }
        _ => Err(error("Not a settlement command")),
    }
}
