use super::*;
pub(super) fn apply(state: &mut State, actor: &Actor, command: &Command) -> Result<String> {
    let revision = next_version(state.revision)?;
    match command {
        Command::ConversionImport { id, name, csv } => create(
            state,
            actor,
            ConversionTerms {
                id: id.clone(),
                name: name.clone(),
                entries: super::csv::import_conversions(csv)?,
            },
        ),
        Command::ConversionCreate { terms } => create(state, actor, terms.clone()),
        Command::ConversionSubmit { id, version } | Command::ConversionWithdraw { id, version } => {
            let book = state
                .conversion_books
                .get_mut(id)
                .ok_or_else(|| error("Conversion book not found"))?;
            ensure(
                book.version == *version && book.owner == actor.name,
                "Only the conversion book owner may change its current version",
            )?;
            let submit = matches!(command, Command::ConversionSubmit { .. });
            ensure(
                if submit {
                    book.status == RecordStatus::Draft
                } else {
                    matches!(book.status, RecordStatus::Draft | RecordStatus::Pending)
                },
                "Submit only Draft; withdraw only Draft or Pending conversion books",
            )?;
            book.version = next_version(book.version)?;
            if submit {
                book.status = RecordStatus::Pending;
                book.submitted_revision = Some(revision);
            } else {
                book.status = RecordStatus::Withdrawn;
                book.withdrawn_revision = Some(revision);
            }
            Ok(format!(
                "{} conversion {id}",
                if submit { "Submitted" } else { "Withdrew" }
            ))
        }
        Command::ConversionDecide {
            id,
            version,
            approve,
            note,
        } => {
            text(note)?;
            let book = state
                .conversion_books
                .get(id)
                .ok_or_else(|| error("Conversion book not found"))?;
            ensure(
                book.version == *version && book.status == RecordStatus::Pending,
                "Decide the current pending conversion book",
            )?;
            ensure(
                actor.role == Role::Approver && book.owner != actor.name,
                "Conversion review requires an independent commercial reviewer",
            )?;
            if *approve {
                state.ensure_conversion_approval(book, state.revision)?;
            }
            let book = state
                .conversion_books
                .get_mut(id)
                .ok_or_else(|| error("Conversion book not found"))?;
            book.version = next_version(book.version)?;
            book.status = if *approve {
                RecordStatus::Approved
            } else {
                RecordStatus::Rejected
            };
            book.review = Some(RecordReview {
                actor: actor.clone(),
                approved: *approve,
                note: note.clone(),
                revision,
            });
            conversion_decided_detail(book)
        }
        Command::ReportConvert { id: key, request } => {
            id(key)?;
            ensure(
                !state.conversion_reports.contains_key(key),
                "Report ID already exists; frozen reports are immutable",
            )?;
            let report = ConversionReport {
                id: key.clone(),
                owner: actor.name.clone(),
                created_revision: revision,
                calculation: state.converted_actuals(request, state.revision)?,
            };
            let detail = report_created_detail(&report)?;
            state.conversion_reports.insert(key.clone(), report);
            Ok(detail)
        }
        _ => Err(error("Not a conversion command")),
    }
}
fn create(state: &mut State, actor: &Actor, terms: ConversionTerms) -> Result<String> {
    state.validate_conversion_terms(&terms)?;
    ensure(
        !state.conversion_books.contains_key(&terms.id),
        "Conversion book ID already exists; terms are immutable",
    )?;
    let book = ConversionBook {
        terms,
        owner: actor.name.clone(),
        created_revision: next_version(state.revision)?,
        submitted_revision: None,
        review: None,
        withdrawn_revision: None,
        version: 1,
        status: RecordStatus::Draft,
    };
    let detail = conversion_created_detail(&book)?;
    state.conversion_books.insert(book.terms.id.clone(), book);
    Ok(detail)
}
