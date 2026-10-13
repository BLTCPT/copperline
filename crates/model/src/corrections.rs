//! Immutable full-invoice corrections. Approval, not a wall clock, activates a reversal.
use super::*;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecordStatus {
    Draft,
    Pending,
    Approved,
    Rejected,
    Withdrawn,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordReview {
    pub actor: Actor,
    pub approved: bool,
    pub note: String,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorrectionProposal {
    pub id: String,
    pub original_transaction_id: String,
    /// Record date only, not scheduled activation. Approval immediately restates
    /// the original invoice's rebate period, regardless of this date.
    pub effective_date: Date,
    pub reason: String,
    pub replacement: Option<Transaction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Correction {
    pub proposal: CorrectionProposal,
    pub original: Transaction,
    pub owner: String,
    pub version: u64,
    pub created_revision: u64,
    pub submitted_revision: Option<u64>,
    pub review: Option<RecordReview>,
    pub withdrawn_revision: Option<u64>,
    pub status: RecordStatus,
}
impl Correction {
    pub fn is_live(&self) -> bool {
        matches!(
            self.status,
            RecordStatus::Draft | RecordStatus::Pending | RecordStatus::Approved
        )
    }
    pub fn approved_revision(&self) -> Option<u64> {
        if self.status == RecordStatus::Approved {
            self.review.as_ref().map(|r| r.revision)
        } else {
            None
        }
    }
    fn released_revision(&self) -> Option<u64> {
        match self.status {
            RecordStatus::Rejected => self.review.as_ref().map(|r| r.revision),
            RecordStatus::Withdrawn => self.withdrawn_revision,
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccrualBasis {
    /// All eligible gross invoice records, including originals later reversed.
    pub transaction_ids: Vec<String>,
    /// Approved full reversals of eligible originals, known at source_revision.
    pub correction_ids: Vec<String>,
    pub gross_invoice: Money,
    pub reversed_invoice: Money,
    pub eligible_invoice: Money,
}

pub fn correction_target(id: &str) -> String {
    format!("correction:{id}")
}
pub fn correction_created_detail(c: &Correction) -> Result<String> {
    let evidence = record_fingerprint(&(&c.proposal, &c.original, &c.owner, c.created_revision))?;
    Ok(format!("Created correction {}: {evidence}", c.proposal.id))
}
pub fn correction_decision_detail(c: &Correction, review: &RecordReview) -> Result<String> {
    let evidence = record_fingerprint(&(correction_created_detail(c)?, review))?;
    Ok(format!(
        "{} correction {}: {evidence}",
        if review.approved {
            "Approved"
        } else {
            "Rejected"
        },
        c.proposal.id
    ))
}

impl State {
    pub fn validate_transaction(&self, t: &Transaction) -> Result<()> {
        id(&t.id)?;
        text(&t.source)?;
        id(&t.product_id)?;
        id(&t.customer_id)?;
        id(&t.unit)?;
        t.date.validate()?;
        t.quantity.validate()?;
        ensure(
            t.invoice_total.0 >= 0 && t.cost_total.0 >= 0,
            "Invoice amounts must be nonnegative; use an approved full correction",
        )?;
        let p = self
            .products
            .get(&t.product_id)
            .ok_or_else(|| error("Transaction product missing"))?;
        ensure(
            t.unit == p.unit
                && t.currency == p.currency
                && self.customers.contains_key(&t.customer_id),
            "Transaction context mismatch",
        )
    }

    /// IDs reserved by pending proposals cannot enter the ordinary import path.
    pub fn transaction_id_available(&self, id: &str) -> bool {
        !self.transactions.contains_key(id)
            && !self.corrections.values().any(|c| {
                matches!(c.status, RecordStatus::Draft | RecordStatus::Pending)
                    && c.proposal.replacement.as_ref().is_some_and(|r| r.id == id)
            })
    }

    pub fn accrual_basis(&self, agreement_id: &str, source_revision: u64) -> Result<AccrualBasis> {
        ensure(
            source_revision <= self.revision,
            "Accrual source is in the future",
        )?;
        let agreement = self
            .agreements
            .get(agreement_id)
            .ok_or_else(|| error("Accrual agreement missing"))?;
        ensure(
            agreement.status == Status::Approved
                && agreement
                    .decisions
                    .last()
                    .is_some_and(|d| d.approved && d.revision <= source_revision),
            "Accrual requires an agreement approved at its source revision",
        )?;
        let mut basis = AccrualBasis {
            transaction_ids: vec![],
            correction_ids: vec![],
            gross_invoice: Money(0),
            reversed_invoice: Money(0),
            eligible_invoice: Money(0),
        };
        for t in self.transactions.values() {
            if t.customer_id == agreement.terms.customer_id
                && t.currency == agreement.terms.currency
                && t.date >= agreement.terms.start
                && t.date < agreement.terms.end
                && self
                    .transaction_revisions
                    .get(&t.id)
                    .is_some_and(|r| *r <= source_revision)
            {
                basis.transaction_ids.push(t.id.clone());
                basis.gross_invoice = basis.gross_invoice.add(t.invoice_total)?;
            }
        }
        let eligible: BTreeSet<_> = basis.transaction_ids.iter().collect();
        let mut reversed = BTreeSet::new();
        for c in self.corrections.values() {
            if c.approved_revision().is_some_and(|r| r <= source_revision)
                && eligible.contains(&c.proposal.original_transaction_id)
            {
                ensure(
                    reversed.insert(&c.proposal.original_transaction_id),
                    "Invoice cannot be reversed twice",
                )?;
                let original = self
                    .transactions
                    .get(&c.proposal.original_transaction_id)
                    .ok_or_else(|| error("Reversed invoice missing"))?;
                basis.correction_ids.push(c.proposal.id.clone());
                basis.reversed_invoice = basis.reversed_invoice.add(original.invoice_total)?;
            }
        }
        basis.eligible_invoice = basis.gross_invoice.sub(basis.reversed_invoice)?;
        ensure(
            basis.eligible_invoice.0 >= 0,
            "Reversed revenue exceeds eligible gross invoices",
        )?;
        Ok(basis)
    }

    fn correction_audit(
        &self,
        c: &Correction,
        revision: u64,
        command: &str,
        detail: &str,
    ) -> Result<&Audit> {
        let a = self
            .audit
            .iter()
            .find(|a| a.revision == revision)
            .ok_or_else(|| error("Correction audit missing"))?;
        ensure(
            a.target.as_deref() == Some(&correction_target(&c.proposal.id))
                && a.command == command
                && a.detail == detail,
            "Correction target, command or outcome audit mismatch",
        )?;
        Ok(a)
    }

    pub(crate) fn validate_corrections(&self) -> Result<()> {
        ensure(self.corrections.len() <= 10_000, "Correction limit reached")?;
        let mut originals: BTreeMap<&str, Vec<&Correction>> = BTreeMap::new();
        let mut replacement_ids: BTreeMap<&str, Vec<&Correction>> = BTreeMap::new();
        for (key, c) in &self.corrections {
            let p = &c.proposal;
            id(&p.id)?;
            id(&p.original_transaction_id)?;
            text(&p.reason)?;
            text(&c.owner)?;
            p.effective_date.validate()?;
            ensure(
                key == &p.id && c.created_revision > 0 && c.created_revision <= self.revision,
                "Correction identity or creation revision invalid",
            )?;
            self.validate_transaction(&c.original)?;
            ensure(
                c.original.id == p.original_transaction_id
                    && self.transactions.get(&p.original_transaction_id) == Some(&c.original),
                "Correction original does not match immutable invoice",
            )?;
            ensure(
                self.transaction_revisions
                    .get(&p.original_transaction_id)
                    .is_some_and(|r| *r < c.created_revision),
                "Correction predates original invoice ingestion",
            )?;
            ensure(
                p.effective_date >= c.original.date,
                "Correction record date precedes original invoice",
            )?;
            let created = self.correction_audit(
                c,
                c.created_revision,
                "correction_create",
                &correction_created_detail(c)?,
            )?;
            ensure(
                created.actor.name == c.owner
                    && matches!(created.actor.role, Role::Analyst | Role::Admin),
                "Correction creator or owner invalid",
            )?;
            let mut events = vec![c.created_revision];
            if let Some(submitted) = c.submitted_revision {
                ensure(
                    submitted > c.created_revision && submitted <= self.revision,
                    "Correction submission chronology invalid",
                )?;
                let a = self.correction_audit(
                    c,
                    submitted,
                    "correction_submit",
                    &format!("Submitted correction {}", p.id),
                )?;
                ensure(
                    a.actor.name == c.owner && matches!(a.actor.role, Role::Analyst | Role::Admin),
                    "Only the correction owner may submit",
                )?;
                events.push(submitted);
            }
            if let Some(review) = &c.review {
                text(&review.actor.name)?;
                text(&review.note)?;
                ensure(
                    c.submitted_revision.is_some_and(|r| review.revision > r)
                        && review.revision <= self.revision,
                    "Correction review chronology invalid",
                )?;
                ensure(
                    review.actor.role == Role::Finance && review.actor.name != c.owner,
                    "Correction requires independent Finance review",
                )?;
                let a = self.correction_audit(
                    c,
                    review.revision,
                    "correction_decide",
                    &correction_decision_detail(c, review)?,
                )?;
                ensure(
                    a.actor == review.actor,
                    "Correction reviewer does not match audit",
                )?;
                events.push(review.revision);
            }
            if let Some(withdrawn) = c.withdrawn_revision {
                ensure(
                    withdrawn > c.submitted_revision.unwrap_or(c.created_revision)
                        && withdrawn <= self.revision,
                    "Correction withdrawal chronology invalid",
                )?;
                let a = self.correction_audit(
                    c,
                    withdrawn,
                    "correction_withdraw",
                    &format!("Withdrew correction {}", p.id),
                )?;
                ensure(
                    a.actor.name == c.owner && matches!(a.actor.role, Role::Analyst | Role::Admin),
                    "Only the correction owner may withdraw",
                )?;
                events.push(withdrawn);
            }
            let valid = match c.status {
                RecordStatus::Draft => {
                    c.version == 1
                        && c.submitted_revision.is_none()
                        && c.review.is_none()
                        && c.withdrawn_revision.is_none()
                }
                RecordStatus::Pending => {
                    c.version == 2
                        && c.submitted_revision.is_some()
                        && c.review.is_none()
                        && c.withdrawn_revision.is_none()
                }
                RecordStatus::Approved | RecordStatus::Rejected => {
                    c.version == 3
                        && c.submitted_revision.is_some()
                        && c.withdrawn_revision.is_none()
                        && c.review
                            .as_ref()
                            .is_some_and(|r| r.approved == (c.status == RecordStatus::Approved))
                }
                RecordStatus::Withdrawn => {
                    c.version == if c.submitted_revision.is_some() { 3 } else { 2 }
                        && c.review.is_none()
                        && c.withdrawn_revision.is_some()
                }
            };
            ensure(valid, "Correction status/version/provenance mismatch")?;
            let audited: Vec<_> = self
                .audit
                .iter()
                .filter(|a| a.target.as_deref() == Some(&correction_target(&p.id)))
                .map(|a| a.revision)
                .collect();
            ensure(
                audited == events,
                "Correction history cannot be discarded or invented",
            )?;
            originals
                .entry(&p.original_transaction_id)
                .or_default()
                .push(c);
            if let Some(r) = &p.replacement {
                self.validate_transaction(r)?;
                ensure(
                    r.id != p.original_transaction_id && r.date <= p.effective_date,
                    "Replacement must have a new ID and cannot postdate correction record date",
                )?;
                replacement_ids.entry(&r.id).or_default().push(c);
                if let Some(revision) = c.approved_revision() {
                    ensure(
                        self.transactions.get(&r.id) == Some(r)
                            && self.transaction_revisions.get(&r.id) == Some(&revision),
                        "Approved replacement was not inserted atomically with its decision",
                    )?;
                } else if let Some(ingested) = self.transaction_revisions.get(&r.id) {
                    ensure(
                        c.released_revision()
                            .is_some_and(|release| *ingested > release),
                        "Replacement ID existed or was used while reserved",
                    )?;
                }
            }
            // Parent replacement must have been approved before this proposal, and
            // record dates cannot move backward along the immutable correction chain.
            if let Some(parent) = self.corrections.values().find(|parent| {
                parent.status == RecordStatus::Approved
                    && parent
                        .proposal
                        .replacement
                        .as_ref()
                        .is_some_and(|r| r.id == p.original_transaction_id)
            }) {
                ensure(
                    parent
                        .approved_revision()
                        .is_some_and(|r| r < c.created_revision)
                        && p.effective_date >= parent.proposal.effective_date,
                    "Correction replacement chain chronology invalid",
                )?;
            }
        }
        for histories in [originals, replacement_ids] {
            for mut history in histories.into_values() {
                history.sort_by_key(|c| c.created_revision);
                for pair in history.windows(2) {
                    ensure(
                        pair[0]
                            .released_revision()
                            .is_some_and(|r| r < pair[1].created_revision),
                        "Overlapping correction or replacement reservations",
                    )?;
                }
            }
        }
        for a in &self.audit {
            if a.command.starts_with("correction_")
                || a.target
                    .as_deref()
                    .is_some_and(|t| t.starts_with("correction:"))
            {
                let key = a
                    .target
                    .as_deref()
                    .and_then(|t| t.strip_prefix("correction:"))
                    .ok_or_else(|| error("Correction audit target missing"))?;
                ensure(
                    self.corrections.contains_key(key),
                    "Correction audit points to a missing record",
                )?;
            }
        }
        Ok(())
    }
}
