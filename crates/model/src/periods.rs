//! Immutable local invoice-date closure; no accounting posting or period carry-forward.
use super::*;
use std::collections::BTreeSet;

const MAX_CLOSES: usize = 1000;
const MAX_BASIS_REFERENCES: usize = 100_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PeriodCloseTerms {
    pub id: String,
    pub start: Date,
    pub end: Date,
    pub currency: Currency,
    pub reason: String,
}
impl PeriodCloseTerms {
    pub fn validate(&self) -> Result<()> {
        id(&self.id)?;
        self.start.validate()?;
        self.end.validate()?;
        text(&self.reason)?;
        ensure(
            self.start < self.end,
            "Period end is exclusive and must follow start",
        )
    }
    pub fn contains(&self, invoice: &Transaction) -> bool {
        invoice.currency == self.currency && self.start <= invoice.date && invoice.date < self.end
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PeriodBasis {
    pub transaction_ids: Vec<String>,
    pub correction_ids: Vec<String>,
    pub credit_ids: Vec<String>,
    pub gross_invoice: Money,
    pub gross_cost: Money,
    pub reversed_invoice: Money,
    pub reversed_cost: Money,
    pub credited_invoice: Money,
    pub credited_cost: Money,
    pub invoice: Money,
    pub cost: Money,
}
impl PeriodBasis {
    fn reference_count(&self) -> Result<usize> {
        self.transaction_ids
            .len()
            .checked_add(self.correction_ids.len())
            .and_then(|count| count.checked_add(self.credit_ids.len()))
            .ok_or_else(|| error("Period basis reference count overflow"))
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PeriodClose {
    pub terms: PeriodCloseTerms,
    pub owner: String,
    pub version: u64,
    pub created_revision: u64,
    pub source_revision: u64,
    pub basis: PeriodBasis,
    pub contributors: BTreeSet<String>,
    pub submitted_revision: Option<u64>,
    pub review: Option<RecordReview>,
    pub withdrawn_revision: Option<u64>,
    pub status: RecordStatus,
}
impl PeriodClose {
    pub fn approved_revision(&self) -> Option<u64> {
        if self.status == RecordStatus::Approved {
            self.review
                .as_ref()
                .filter(|r| r.approved)
                .map(|r| r.revision)
        } else {
            None
        }
    }
    /// Historically unresolved, even when its eventual decision is now known.
    pub fn pending_at(&self, revision: u64) -> bool {
        unresolved_at(
            self.created_revision,
            self.review.as_ref(),
            self.withdrawn_revision,
            revision,
        )
    }
}
fn unresolved_at(
    created: u64,
    review: Option<&RecordReview>,
    withdrawn: Option<u64>,
    revision: u64,
) -> bool {
    created <= revision
        && review.is_none_or(|r| r.revision > revision)
        && withdrawn.is_none_or(|r| r > revision)
}
pub fn period_close_target(id: &str) -> String {
    format!("period_close:{id}")
}
pub fn period_close_created_detail(record: &PeriodClose) -> Result<String> {
    let hash = record_fingerprint(&(
        &record.terms,
        &record.owner,
        record.created_revision,
        record.source_revision,
        &record.basis,
        &record.contributors,
    ))?;
    Ok(format!("Created period close {}: {hash}", record.terms.id))
}
pub fn period_close_decided_detail(record: &PeriodClose) -> Result<String> {
    let review = record
        .review
        .as_ref()
        .ok_or_else(|| error("Period close review missing"))?;
    let hash = record_fingerprint(&(period_close_created_detail(record)?, review))?;
    Ok(format!("Reviewed period close {}: {hash}", record.terms.id))
}
impl State {
    fn period_reference_count(&self) -> Result<usize> {
        self.period_closes
            .values()
            .try_fold(0_usize, |count, close| {
                count
                    .checked_add(close.basis.reference_count()?)
                    .ok_or_else(|| error("Period basis reference count overflow"))
            })
    }
    pub fn period_basis(&self, terms: &PeriodCloseTerms, as_of: u64) -> Result<PeriodBasis> {
        terms.validate()?;
        ensure(
            as_of <= self.revision,
            "Period basis revision is in the future",
        )?;
        let mut basis = PeriodBasis {
            transaction_ids: vec![],
            correction_ids: vec![],
            credit_ids: vec![],
            gross_invoice: Money(0),
            gross_cost: Money(0),
            reversed_invoice: Money(0),
            reversed_cost: Money(0),
            credited_invoice: Money(0),
            credited_cost: Money(0),
            invoice: Money(0),
            cost: Money(0),
        };
        for (id, invoice) in &self.transactions {
            if terms.contains(invoice) {
                let revision = self
                    .transaction_revisions
                    .get(id)
                    .ok_or_else(|| error("Period invoice ingestion provenance missing"))?;
                if *revision <= as_of {
                    basis.transaction_ids.push(id.clone());
                    basis.gross_invoice = basis.gross_invoice.add(invoice.invoice_total)?;
                    basis.gross_cost = basis.gross_cost.add(invoice.cost_total)?;
                }
            }
        }
        for (id, correction) in &self.corrections {
            if correction.approved_revision().is_some_and(|r| r <= as_of) {
                let original = terms.contains(&correction.original);
                let replacement = correction
                    .proposal
                    .replacement
                    .as_ref()
                    .is_some_and(|r| terms.contains(r));
                if original || replacement {
                    basis.correction_ids.push(id.clone());
                }
                if original {
                    basis.reversed_invoice = basis
                        .reversed_invoice
                        .add(correction.original.invoice_total)?;
                    basis.reversed_cost =
                        basis.reversed_cost.add(correction.original.cost_total)?;
                }
            }
        }
        for (id, credit) in &self.credits {
            if credit.approved_revision().is_some_and(|r| r <= as_of)
                && terms.contains(&credit.original)
            {
                basis.credit_ids.push(id.clone());
                basis.credited_invoice = basis.credited_invoice.add(credit.invoice)?;
                basis.credited_cost = basis.credited_cost.add(credit.cost)?;
            }
        }
        basis.invoice = basis
            .gross_invoice
            .sub(basis.reversed_invoice)?
            .sub(basis.credited_invoice)?;
        basis.cost = basis
            .gross_cost
            .sub(basis.reversed_cost)?
            .sub(basis.credited_cost)?;
        ensure(
            basis.invoice.0 >= 0 && basis.cost.0 >= 0,
            "Period reductions exceed original invoices",
        )?;
        Ok(basis)
    }
    /// Only approvals already known at the attempted action can close its invoice date.
    pub fn ensure_invoice_open_at(&self, invoice: &Transaction, as_of: u64) -> Result<()> {
        ensure(
            as_of <= self.revision,
            "Invoice openness revision is in the future",
        )?;
        for close in self.period_closes.values() {
            if close.approved_revision().is_some_and(|r| r <= as_of)
                && close.terms.contains(invoice)
            {
                return Err(error(format!(
                    "Invoice date/currency belongs to closed period {}; no carry-forward or reopening is supported",
                    close.terms.id
                )));
            }
        }
        Ok(())
    }
    fn period_contributors(&self, basis: &PeriodBasis, owner: &str) -> Result<BTreeSet<String>> {
        let mut names = BTreeSet::from([owner.to_owned()]);
        for id in &basis.transaction_ids {
            if let Some((booking, _)) = self.booking_for_transaction(id) {
                names.insert(booking.owner.clone());
                names.extend(booking.quote_contributors.iter().cloned());
            }
            if let Some(correction) = self.corrections.values().find(|c| {
                c.approved_revision().is_some()
                    && c.proposal.replacement.as_ref().is_some_and(|r| &r.id == id)
            }) {
                // A generated replacement's commercial author is its proposer;
                // the independent Finance insertion reviewer is not its author.
                names.insert(correction.owner.clone());
            } else {
                let revision = self
                    .transaction_revisions
                    .get(id)
                    .ok_or_else(|| error("Period invoice author missing"))?;
                let event = self
                    .audit
                    .iter()
                    .find(|a| a.revision == *revision)
                    .ok_or_else(|| error("Period invoice audit missing"))?;
                names.insert(event.actor.name.clone());
            }
        }
        for id in &basis.correction_ids {
            names.insert(
                self.corrections
                    .get(id)
                    .ok_or_else(|| error("Period correction missing"))?
                    .owner
                    .clone(),
            );
        }
        for id in &basis.credit_ids {
            names.insert(
                self.credits
                    .get(id)
                    .ok_or_else(|| error("Period credit missing"))?
                    .owner
                    .clone(),
            );
        }
        Ok(names)
    }
    pub fn ensure_period_close_current(&self, close: &PeriodClose, as_of: u64) -> Result<()> {
        ensure(
            close.basis == self.period_basis(&close.terms, as_of)?,
            "Period basis changed; withdraw and create a new close proposal",
        )?;
        for correction in self.corrections.values() {
            if unresolved_at(
                correction.created_revision,
                correction.review.as_ref(),
                correction.withdrawn_revision,
                as_of,
            ) && (close.terms.contains(&correction.original)
                || correction
                    .proposal
                    .replacement
                    .as_ref()
                    .is_some_and(|r| close.terms.contains(r)))
            {
                return Err(error(
                    "Period has an unresolved full correction affecting its original or replacement invoices",
                ));
            }
        }
        for credit in self.credits.values() {
            if unresolved_at(
                credit.created_revision,
                credit.review.as_ref(),
                credit.withdrawn_revision,
                as_of,
            ) && close.terms.contains(&credit.original)
            {
                return Err(error("Period has an unresolved partial credit"));
            }
        }
        for other in self.period_closes.values() {
            if other.terms.id != close.terms.id
                && other.approved_revision().is_some_and(|r| r <= as_of)
                && other.terms.currency == close.terms.currency
                && other.terms.start < close.terms.end
                && close.terms.start < other.terms.end
            {
                return Err(error(
                    "Period overlaps an approved close in the same currency",
                ));
            }
        }
        Ok(())
    }
    pub fn period_close_preview(
        &self,
        terms: &PeriodCloseTerms,
        owner: &str,
    ) -> Result<PeriodClose> {
        terms.validate()?;
        text(owner)?;
        ensure(
            !self.period_closes.contains_key(&terms.id),
            "Period close ID already exists",
        )?;
        ensure(
            self.period_closes.len() < MAX_CLOSES,
            "Period close limit reached",
        )?;
        let basis = self.period_basis(terms, self.revision)?;
        let references = self
            .period_reference_count()?
            .checked_add(basis.reference_count()?)
            .ok_or_else(|| error("Period basis reference count overflow"))?;
        ensure(
            references <= MAX_BASIS_REFERENCES,
            "Period close limit is 100,000 combined frozen basis IDs",
        )?;
        let close = PeriodClose {
            terms: terms.clone(),
            owner: owner.into(),
            version: 1,
            created_revision: self
                .revision
                .checked_add(1)
                .ok_or_else(|| error("Period revision overflow"))?,
            source_revision: self.revision,
            contributors: self.period_contributors(&basis, owner)?,
            basis,
            submitted_revision: None,
            review: None,
            withdrawn_revision: None,
            status: RecordStatus::Draft,
        };
        self.ensure_period_close_current(&close, self.revision)?;
        Ok(close)
    }
    fn period_event(
        &self,
        close: &PeriodClose,
        revision: u64,
        command: &str,
        detail: &str,
    ) -> Result<&Audit> {
        let event = self
            .audit
            .iter()
            .find(|a| a.revision == revision)
            .ok_or_else(|| error("Period close audit missing"))?;
        ensure(
            event.command == command
                && event.target.as_deref() == Some(&period_close_target(&close.terms.id))
                && event.detail == detail,
            "Period close command, target or evidence audit mismatch",
        )?;
        Ok(event)
    }
    pub(crate) fn validate_period_closes(&self) -> Result<()> {
        ensure(
            self.period_closes.len() <= MAX_CLOSES,
            "Period close limit reached",
        )?;
        ensure(
            self.period_reference_count()? <= MAX_BASIS_REFERENCES,
            "Period close limit is 100,000 combined frozen basis IDs",
        )?;
        for (key, close) in &self.period_closes {
            close.terms.validate()?;
            text(&close.owner)?;
            ensure(
                key == &close.terms.id
                    && close.created_revision > 0
                    && close.created_revision <= self.revision
                    && close.source_revision.checked_add(1) == Some(close.created_revision),
                "Period close identity or chronology invalid",
            )?;
            ensure(
                close.contributors == self.period_contributors(&close.basis, &close.owner)?,
                "Period close contributor evidence mismatch",
            )?;
            self.ensure_period_close_current(close, close.source_revision)?;
            let created = self.period_event(
                close,
                close.created_revision,
                "period_close_create",
                &period_close_created_detail(close)?,
            )?;
            ensure(
                created.actor.name == close.owner
                    && matches!(created.actor.role, Role::Analyst | Role::Admin),
                "Period close owner or creator role invalid",
            )?;
            let mut expected = vec![close.created_revision];
            if let Some(revision) = close.submitted_revision {
                ensure(
                    revision > close.created_revision && revision <= self.revision,
                    "Period close submission chronology invalid",
                )?;
                self.ensure_period_close_current(
                    close,
                    revision
                        .checked_sub(1)
                        .ok_or_else(|| error("Invalid period submission revision"))?,
                )?;
                let event = self.period_event(
                    close,
                    revision,
                    "period_close_submit",
                    &format!("Submitted period close {key}"),
                )?;
                ensure(
                    event.actor.name == close.owner
                        && matches!(event.actor.role, Role::Analyst | Role::Admin),
                    "Only the period close owner may submit",
                )?;
                expected.push(revision);
            }
            if let Some(review) = &close.review {
                text(&review.actor.name)?;
                text(&review.note)?;
                ensure(
                    close
                        .submitted_revision
                        .is_some_and(|r| r < review.revision)
                        && review.revision <= self.revision,
                    "Period close review chronology invalid",
                )?;
                ensure(
                    review.actor.role == Role::Finance
                        && !close.contributors.contains(&review.actor.name),
                    "Period close requires independent Finance review",
                )?;
                let event = self.period_event(
                    close,
                    review.revision,
                    "period_close_decide",
                    &period_close_decided_detail(close)?,
                )?;
                ensure(
                    event.actor == review.actor,
                    "Period close reviewer differs from audit",
                )?;
                if review.approved {
                    self.ensure_period_close_current(
                        close,
                        review
                            .revision
                            .checked_sub(1)
                            .ok_or_else(|| error("Invalid period review revision"))?,
                    )?;
                }
                expected.push(review.revision);
            }
            if let Some(revision) = close.withdrawn_revision {
                ensure(
                    revision > close.submitted_revision.unwrap_or(close.created_revision)
                        && revision <= self.revision,
                    "Period close withdrawal chronology invalid",
                )?;
                let event = self.period_event(
                    close,
                    revision,
                    "period_close_withdraw",
                    &format!("Withdrew period close {key}"),
                )?;
                ensure(
                    event.actor.name == close.owner
                        && matches!(event.actor.role, Role::Analyst | Role::Admin),
                    "Only the period close owner may withdraw",
                )?;
                expected.push(revision);
            }
            let lifecycle = match close.status {
                RecordStatus::Draft => {
                    close.version == 1
                        && close.submitted_revision.is_none()
                        && close.review.is_none()
                        && close.withdrawn_revision.is_none()
                }
                RecordStatus::Pending => {
                    close.version == 2
                        && close.submitted_revision.is_some()
                        && close.review.is_none()
                        && close.withdrawn_revision.is_none()
                }
                RecordStatus::Approved | RecordStatus::Rejected => {
                    close.version == 3
                        && close.submitted_revision.is_some()
                        && close.withdrawn_revision.is_none()
                        && close
                            .review
                            .as_ref()
                            .is_some_and(|r| r.approved == (close.status == RecordStatus::Approved))
                }
                RecordStatus::Withdrawn => {
                    close.version
                        == if close.submitted_revision.is_some() {
                            3
                        } else {
                            2
                        }
                        && close.review.is_none()
                        && close.withdrawn_revision.is_some()
                }
            };
            ensure(lifecycle, "Period close lifecycle/version invalid")?;
            let target = period_close_target(key);
            let actual: Vec<_> = self
                .audit
                .iter()
                .filter(|a| a.target.as_deref() == Some(&target))
                .map(|a| a.revision)
                .collect();
            ensure(
                actual == expected,
                "Period close history cannot be erased or invented",
            )?;
        }
        for event in self.audit.iter().filter(|a| {
            a.command.starts_with("period_close_")
                || a.target
                    .as_deref()
                    .is_some_and(|t| t.starts_with("period_close:"))
        }) {
            let key = event
                .target
                .as_deref()
                .and_then(|t| t.strip_prefix("period_close:"))
                .ok_or_else(|| error("Period close target missing"))?;
            ensure(
                self.period_closes.contains_key(key),
                "Period close evidence was removed",
            )?;
        }
        Ok(())
    }
}
