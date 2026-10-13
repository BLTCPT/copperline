use super::*;
pub(super) fn apply(state: &mut State, actor: &Actor, command: &Command) -> Result<String> {
    let revision = next_version(state.revision)?;
    match command {
        Command::ContractCreate { terms } => {
            ensure(
                terms.edition == next_version(state.latest_contract_edition(&terms.id))?,
                "Create the next contract edition; earlier editions remain immutable",
            )?;
            if let Some(first) = state
                .price_contracts
                .values()
                .find(|c| c.terms.id == terms.id)
            {
                ensure(
                    first.owner == actor.name
                        && first.terms.customer_id == terms.customer_id
                        && first.terms.currency == terms.currency,
                    "Contract family retains its owner, customer and currency",
                )?;
            }
            let rows = state.freeze_contract_rows(terms, state.revision, false)?;
            let record = PriceContract {
                terms: terms.clone(),
                contributors: state.contract_contributors(&rows, &actor.name)?,
                rows,
                owner: actor.name.clone(),
                created_revision: revision,
                submitted_revision: None,
                review: None,
                withdrawn_revision: None,
                version: 1,
                status: RecordStatus::Draft,
            };
            let detail = contract_created_detail(&record)?;
            state
                .price_contracts
                .insert(contract_key(&terms.id, terms.edition), record);
            Ok(detail)
        }
        Command::ContractSubmit {
            id,
            edition,
            version,
        }
        | Command::ContractWithdraw {
            id,
            edition,
            version,
        } => {
            let record = state.contract(id, *edition)?;
            ensure(
                record.version == *version && record.owner == actor.name,
                "Only the contract owner may change its current version",
            )?;
            let submit = matches!(command, Command::ContractSubmit { .. });
            ensure(
                if submit {
                    record.status == RecordStatus::Draft
                } else {
                    matches!(record.status, RecordStatus::Draft | RecordStatus::Pending)
                },
                "Submit only Draft; withdraw only Draft or Pending contracts",
            )?;
            let record = state
                .price_contracts
                .get_mut(&contract_key(id, *edition))
                .ok_or_else(|| error("Contract not found"))?;
            record.version = next_version(record.version)?;
            if submit {
                record.status = RecordStatus::Pending;
                record.submitted_revision = Some(revision);
            } else {
                record.status = RecordStatus::Withdrawn;
                record.withdrawn_revision = Some(revision);
            }
            Ok(format!(
                "{} contract {id}",
                if submit { "Submitted" } else { "Withdrew" }
            ))
        }
        Command::ContractDecide {
            id,
            edition,
            version,
            approve,
            note,
        } => {
            text(note)?;
            let record = state.contract(id, *edition)?;
            ensure(
                record.version == *version && record.status == RecordStatus::Pending,
                "Decide the current pending contract edition",
            )?;
            ensure(
                actor.role == Role::Approver && !record.contributors.contains(&actor.name),
                "Contract review requires an independent commercial reviewer, excluding origin authors",
            )?;
            if *approve {
                state.ensure_contract_approval(record, state.revision)?;
            }
            let record = state
                .price_contracts
                .get_mut(&contract_key(id, *edition))
                .ok_or_else(|| error("Contract not found"))?;
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
            contract_decided_detail(record)
        }
        _ => Err(error("Not a price contract command")),
    }
}
