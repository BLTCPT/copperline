use super::*;

pub(super) fn apply(s: &mut State, actor: &Actor, c: &Command) -> Result<String> {
    match c {
        Command::CorrectionCreate { proposal } => {
            ensure(
                matches!(actor.role, Role::Analyst | Role::Admin),
                "Only an analyst or admin may propose a correction",
            )?;
            id(&proposal.id)?;
            id(&proposal.original_transaction_id)?;
            text(&proposal.reason)?;
            proposal.effective_date.validate()?;
            ensure(
                !s.corrections.contains_key(&proposal.id),
                "Correction ID already exists; proposals are immutable",
            )?;
            ensure(
                !s.corrections.values().any(|c| {
                    c.is_live()
                        && c.proposal.original_transaction_id == proposal.original_transaction_id
                }),
                "Original already has a live correction or was reversed",
            )?;
            let original = s
                .transactions
                .get(&proposal.original_transaction_id)
                .ok_or_else(|| error("Original invoice not found"))?
                .clone();
            ensure(
                s.booking_for_transaction(&original.id).is_none() || proposal.replacement.is_none(),
                "Booked invoices support full reversal only; use a distinct approved quote for replacement",
            )?;
            ensure(
                proposal.effective_date >= original.date,
                "Correction record date precedes original invoice",
            )?;
            if let Some(replacement) = &proposal.replacement {
                s.validate_transaction(replacement)?;
                ensure(
                    s.transaction_id_available(&replacement.id),
                    "Replacement external ID already exists or is reserved",
                )?;
                ensure(
                    replacement.date <= proposal.effective_date,
                    "Replacement invoice postdates correction record date",
                )?;
            }
            let record = Correction {
                proposal: proposal.clone(),
                original,
                owner: actor.name.clone(),
                version: 1,
                created_revision: next_version(s.revision)?,
                submitted_revision: None,
                review: None,
                withdrawn_revision: None,
                status: RecordStatus::Draft,
            };
            let detail = correction_created_detail(&record)?;
            s.corrections.insert(proposal.id.clone(), record);
            Ok(detail)
        }
        Command::CorrectionSubmit { id, version } => {
            let revision = next_version(s.revision)?;
            let c = s
                .corrections
                .get_mut(id)
                .ok_or_else(|| error("Correction not found"))?;
            owner(c, actor)?;
            ensure(
                c.version == *version && c.status == RecordStatus::Draft,
                "Submit the current draft correction version",
            )?;
            c.status = RecordStatus::Pending;
            c.version = next_version(c.version)?;
            c.submitted_revision = Some(revision);
            Ok(format!("Submitted correction {id}"))
        }
        Command::CorrectionDecide {
            id,
            version,
            approve,
            note,
        } => {
            text(note)?;
            ensure(
                actor.role == Role::Finance,
                "Correction decisions require Finance; Admin cannot substitute",
            )?;
            let revision = next_version(s.revision)?;
            let c = s
                .corrections
                .get(id)
                .ok_or_else(|| error("Correction not found"))?;
            ensure(
                c.version == *version && c.status == RecordStatus::Pending,
                "Decide the current pending correction version",
            )?;
            ensure(
                actor.name != c.owner,
                "The correction owner cannot review their own proposal",
            )?;
            let replacement = if *approve {
                c.proposal.replacement.clone()
            } else {
                None
            };
            if let Some(replacement) = &replacement {
                ensure(
                    !s.transactions.contains_key(&replacement.id),
                    "Replacement external ID already exists",
                )?;
                ensure(
                    !s.corrections.values().any(|other| {
                        other.proposal.id != *id
                            && other.is_live()
                            && other
                                .proposal
                                .replacement
                                .as_ref()
                                .is_some_and(|t| t.id == replacement.id)
                    }),
                    "Replacement external ID is reserved by another proposal",
                )?;
            }
            let c = s
                .corrections
                .get_mut(id)
                .ok_or_else(|| error("Correction not found"))?;
            c.status = if *approve {
                RecordStatus::Approved
            } else {
                RecordStatus::Rejected
            };
            c.version = next_version(c.version)?;
            let review = RecordReview {
                actor: actor.clone(),
                approved: *approve,
                note: note.clone(),
                revision,
            };
            let detail = correction_decision_detail(c, &review)?;
            c.review = Some(review);
            if let Some(replacement) = replacement {
                s.transaction_revisions
                    .insert(replacement.id.clone(), revision);
                s.transactions.insert(replacement.id.clone(), replacement);
            }
            // Checked gross and net analytics is part of the same candidate transaction.
            super::commercial::analytics(s)?;
            Ok(detail)
        }
        Command::CorrectionWithdraw { id, version } => {
            let revision = next_version(s.revision)?;
            let c = s
                .corrections
                .get_mut(id)
                .ok_or_else(|| error("Correction not found"))?;
            owner(c, actor)?;
            ensure(
                c.version == *version
                    && matches!(c.status, RecordStatus::Draft | RecordStatus::Pending),
                "Withdraw the current draft or pending correction version",
            )?;
            c.status = RecordStatus::Withdrawn;
            c.version = next_version(c.version)?;
            c.withdrawn_revision = Some(revision);
            Ok(format!("Withdrew correction {id}"))
        }
        _ => Err(error("Not a correction command")),
    }
}
fn owner(c: &Correction, actor: &Actor) -> Result<()> {
    ensure(
        matches!(actor.role, Role::Analyst | Role::Admin) && c.owner == actor.name,
        "Only the correction owner may submit or withdraw; Admin does not bypass ownership",
    )
}
