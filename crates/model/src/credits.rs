//! Quantity credits allocate immutable original totals; approval activates the credit.
use super::*;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreditProposal {
    pub id: String,
    pub original_transaction_id: String,
    /// Record date only. Approval immediately restates the original invoice period.
    pub effective_date: Date,
    pub reason: String,
    /// Positive quantity in the immutable recorded invoice unit.
    pub quantity: Quantity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CreditRounding {
    CumulativeOriginalTotalsHalfAwayV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreditBookingEvidence {
    pub booking_id: String,
    pub booking_revision: u64,
    pub booking_fingerprint: String,
    pub line_index: usize,
    /// Complete original non-posted pricing basis, never substituted for actuals.
    pub native_basis: NativeBookingBasis,
    /// Exact native thousandths for this credit; sub-thousandth returns are rejected.
    pub native_quantity: Quantity,
    pub native_invoice: Money,
    pub native_cost: Money,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Credit {
    pub proposal: CreditProposal,
    pub original: Transaction,
    pub original_revision: u64,
    /// Approval order, not lexical order. Only these approved credits consume the baseline.
    pub prior_credit_ids: Vec<String>,
    /// Zero is valid for a first credit, unlike a requested quantity.
    pub prior_quantity: Quantity,
    pub invoice: Money,
    pub cost: Money,
    pub rounding: CreditRounding,
    pub contributors: BTreeSet<String>,
    pub booking: Option<CreditBookingEvidence>,
    pub owner: String,
    pub version: u64,
    pub created_revision: u64,
    pub submitted_revision: Option<u64>,
    pub review: Option<RecordReview>,
    pub withdrawn_revision: Option<u64>,
    pub status: RecordStatus,
}

impl Credit {
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

    /// Whether this proposal was unresolved (Draft/Pending) at this revision.
    /// Already approved credits consume capacity but are not unresolved reservations.
    pub fn live_at(&self, as_of: u64) -> bool {
        self.created_revision <= as_of
            && self.review.as_ref().is_none_or(|r| r.revision > as_of)
            && self.withdrawn_revision.is_none_or(|r| r > as_of)
    }
}

/// Difference of cumulative original-total allocations; final exhaustion clears all cents.
pub fn credit_allocation(
    total: Money,
    original_quantity: Quantity,
    prior: Quantity,
    quantity: Quantity,
) -> Result<Money> {
    original_quantity.validate()?;
    quantity.validate()?;
    ensure(total.0 >= 0, "Credit original total must be nonnegative")?;
    let cumulative = prior
        .0
        .checked_add(quantity.0)
        .ok_or_else(|| error("Cumulative credit quantity overflow"))?;
    ensure(
        prior.0 >= 0 && cumulative <= original_quantity.0,
        "Credit quantity exceeds the original remaining quantity",
    )?;
    let before = copperline_money::ratio(total.0, prior.0, original_quantity.0)?;
    let after = copperline_money::ratio(total.0, cumulative, original_quantity.0)?;
    Money(after).sub(Money(before))
}

pub fn credit_target(id: &str) -> String {
    format!("credit:{id}")
}

pub fn credit_created_detail(c: &Credit) -> Result<String> {
    // Canonical draft form contains every immutable field, including contributors and booking.
    let evidence = record_fingerprint(&credit_immutable(c))?;
    Ok(format!("Created credit {}: {evidence}", c.proposal.id))
}

pub fn credit_decision_detail(c: &Credit, review: &RecordReview) -> Result<String> {
    let evidence = record_fingerprint(&(credit_created_detail(c)?, review))?;
    Ok(format!(
        "{} credit {}: {evidence}",
        if review.approved {
            "Approved"
        } else {
            "Rejected"
        },
        c.proposal.id
    ))
}

fn credit_immutable(c: &Credit) -> Credit {
    let mut value = c.clone();
    value.version = 1;
    value.submitted_revision = None;
    value.review = None;
    value.withdrawn_revision = None;
    value.status = RecordStatus::Draft;
    value
}

fn correction_blocks_at(c: &Correction, as_of: u64) -> bool {
    c.created_revision <= as_of
        && !c
            .review
            .as_ref()
            .is_some_and(|r| !r.approved && r.revision <= as_of)
        && c.withdrawn_revision.is_none_or(|r| r > as_of)
}

impl State {
    pub fn approved_credits_for(&self, original_id: &str, as_of: u64) -> Vec<&Credit> {
        let mut records: Vec<_> = self
            .credits
            .values()
            .filter(|c| {
                c.proposal.original_transaction_id == original_id
                    && c.approved_revision().is_some_and(|r| r <= as_of)
            })
            .collect();
        records.sort_by_key(|c| c.approved_revision());
        records
    }

    fn credit_baseline(&self, original_id: &str, as_of: u64) -> Result<(Vec<String>, Quantity)> {
        ensure(
            as_of <= self.revision,
            "Credit source revision is in the future",
        )?;
        let mut ids = Vec::new();
        let mut quantity = 0_i64;
        for c in self.approved_credits_for(original_id, as_of) {
            c.proposal.quantity.validate()?;
            quantity = quantity
                .checked_add(c.proposal.quantity.0)
                .ok_or_else(|| error("Prior credit quantity overflow"))?;
            ids.push(c.proposal.id.clone());
        }
        Ok((ids, Quantity(quantity)))
    }

    fn credit_available_at(&self, original_id: &str, own_id: &str, as_of: u64) -> Result<()> {
        ensure(
            !self.corrections.values().any(|c| {
                c.proposal.original_transaction_id == original_id && correction_blocks_at(c, as_of)
            }),
            "An unresolved or approved full correction prevents partial credits",
        )?;
        ensure(
            !self.credits.values().any(|c| {
                c.proposal.id != own_id
                    && c.proposal.original_transaction_id == original_id
                    && c.live_at(as_of)
            }),
            "Another unresolved credit reserves this original invoice",
        )
    }

    fn derive_credit(&self, proposal: &CreditProposal, owner: &str, as_of: u64) -> Result<Credit> {
        id(&proposal.id)?;
        id(&proposal.original_transaction_id)?;
        text(&proposal.reason)?;
        text(owner)?;
        proposal.effective_date.validate()?;
        proposal.quantity.validate()?;
        ensure(
            as_of <= self.revision,
            "Credit source revision is in the future",
        )?;
        let created_revision = as_of
            .checked_add(1)
            .ok_or_else(|| error("Credit revision overflow"))?;
        let original = self
            .transactions
            .get(&proposal.original_transaction_id)
            .ok_or_else(|| error("Credit original invoice not found"))?;
        self.validate_recorded_transaction(original)?;
        let original_revision = *self
            .transaction_revisions
            .get(&original.id)
            .ok_or_else(|| error("Credit original ingestion revision missing"))?;
        ensure(
            original_revision > 0 && original_revision <= as_of,
            "Credit predates original invoice ingestion",
        )?;
        ensure(
            proposal.effective_date >= original.date,
            "Credit record date precedes original invoice",
        )?;
        self.credit_available_at(&original.id, &proposal.id, as_of)?;
        self.ensure_invoice_open_at(original, as_of)?;
        let (prior_credit_ids, prior_quantity) = self.credit_baseline(&original.id, as_of)?;
        let invoice = credit_allocation(
            original.invoice_total,
            original.quantity,
            prior_quantity,
            proposal.quantity,
        )?;
        let cost = credit_allocation(
            original.cost_total,
            original.quantity,
            prior_quantity,
            proposal.quantity,
        )?;
        let mut contributors = BTreeSet::from([owner.to_owned()]);
        let ingestion = self
            .audit
            .iter()
            .find(|a| a.revision == original_revision)
            .ok_or_else(|| error("Credit original ingestion audit missing"))?;
        contributors.insert(ingestion.actor.name.clone());
        for correction in self.corrections.values().filter(|c| {
            c.approved_revision().is_some_and(|r| r <= as_of)
                && c.proposal
                    .replacement
                    .as_ref()
                    .is_some_and(|t| t.id == original.id)
        }) {
            contributors.insert(correction.owner.clone());
        }
        for credit in self.approved_credits_for(&original.id, as_of) {
            contributors.insert(credit.owner.clone());
        }
        let booking = if let Some((booking, line)) = self.booking_for_transaction(&original.id) {
            ensure(
                booking.created_revision == original_revision,
                "Credit booking revision differs from invoice ingestion",
            )?;
            contributors.insert(booking.owner.clone());
            contributors.extend(booking.quote_contributors.iter().cloned());
            line.native_basis.quantity.validate()?;
            let numerator = i128::from(line.native_basis.quantity.0)
                .checked_mul(i128::from(proposal.quantity.0))
                .ok_or_else(|| error("Credit native quantity overflow"))?;
            let denominator = i128::from(original.quantity.0);
            ensure(
                numerator % denominator == 0,
                "Credit native quantity must be an exact thousandth",
            )?;
            let native_quantity = Quantity(
                i64::try_from(numerator / denominator)
                    .map_err(|_| error("Credit native quantity overflow"))?,
            );
            native_quantity.validate()?;
            Some(CreditBookingEvidence {
                booking_id: booking.request.id.clone(),
                booking_revision: booking.created_revision,
                booking_fingerprint: record_fingerprint(booking)?,
                line_index: line.index,
                native_basis: line.native_basis.clone(),
                native_quantity,
                native_invoice: credit_allocation(
                    line.native_basis.invoice_total,
                    original.quantity,
                    prior_quantity,
                    proposal.quantity,
                )?,
                native_cost: credit_allocation(
                    line.native_basis.cost_total,
                    original.quantity,
                    prior_quantity,
                    proposal.quantity,
                )?,
            })
        } else {
            None
        };
        Ok(Credit {
            proposal: proposal.clone(),
            original: original.clone(),
            original_revision,
            prior_credit_ids,
            prior_quantity,
            invoice,
            cost,
            rounding: CreditRounding::CumulativeOriginalTotalsHalfAwayV1,
            contributors,
            booking,
            owner: owner.to_owned(),
            version: 1,
            created_revision,
            submitted_revision: None,
            review: None,
            withdrawn_revision: None,
            status: RecordStatus::Draft,
        })
    }

    pub fn credit_preview(&self, proposal: &CreditProposal, owner: &str) -> Result<Credit> {
        ensure(
            !self.credits.contains_key(&proposal.id),
            "Credit ID already exists",
        )?;
        self.derive_credit(proposal, owner, self.revision)
    }

    /// Exact approved baseline, reservation and period checks at a submission/approval boundary.
    pub fn ensure_credit_current(&self, credit: &Credit, as_of: u64) -> Result<()> {
        let expected = self.derive_credit(&credit.proposal, &credit.owner, as_of)?;
        let mut expected = expected;
        expected.created_revision = credit.created_revision;
        ensure(
            credit_immutable(credit) == expected,
            "Credit baseline or original evidence changed; withdraw and create a new proposal",
        )
    }

    fn credit_audit(
        &self,
        c: &Credit,
        revision: u64,
        command: &str,
        detail: &str,
    ) -> Result<&Audit> {
        let audit = self
            .audit
            .iter()
            .find(|a| a.revision == revision)
            .ok_or_else(|| error("Credit audit missing"))?;
        ensure(
            audit.target.as_deref() == Some(&credit_target(&c.proposal.id))
                && audit.command == command
                && audit.detail == detail,
            "Credit target, command or evidence audit mismatch",
        )?;
        Ok(audit)
    }

    pub(crate) fn validate_credits(&self) -> Result<()> {
        ensure(self.credits.len() <= 10_000, "Credit limit reached")?;
        for (key, c) in &self.credits {
            ensure(
                key == &c.proposal.id
                    && c.created_revision > 0
                    && c.created_revision <= self.revision,
                "Credit identity or creation revision invalid",
            )?;
            let expected = self.derive_credit(&c.proposal, &c.owner, c.created_revision - 1)?;
            ensure(
                credit_immutable(c) == expected,
                "Credit immutable original, baseline, allocation or contributor evidence mismatch",
            )?;
            let created = self.credit_audit(
                c,
                c.created_revision,
                "credit_create",
                &credit_created_detail(c)?,
            )?;
            ensure(
                created.actor.name == c.owner
                    && matches!(created.actor.role, Role::Analyst | Role::Admin),
                "Credit creator or owner invalid",
            )?;
            let mut events = vec![c.created_revision];
            if let Some(submitted) = c.submitted_revision {
                ensure(
                    submitted > c.created_revision && submitted <= self.revision,
                    "Credit submission chronology invalid",
                )?;
                let audit = self.credit_audit(
                    c,
                    submitted,
                    "credit_submit",
                    &format!("Submitted credit {}", c.proposal.id),
                )?;
                ensure(
                    audit.actor.name == c.owner
                        && matches!(audit.actor.role, Role::Analyst | Role::Admin),
                    "Only the credit owner may submit",
                )?;
                self.ensure_credit_current(c, submitted - 1)?;
                events.push(submitted);
            }
            if let Some(review) = &c.review {
                text(&review.actor.name)?;
                text(&review.note)?;
                ensure(
                    c.submitted_revision.is_some_and(|r| review.revision > r)
                        && review.revision <= self.revision,
                    "Credit review chronology invalid",
                )?;
                ensure(
                    review.actor.role == Role::Finance
                        && !c.contributors.contains(&review.actor.name),
                    "Credit requires independent non-contributor Finance review",
                )?;
                let audit = self.credit_audit(
                    c,
                    review.revision,
                    "credit_decide",
                    &credit_decision_detail(c, review)?,
                )?;
                ensure(
                    audit.actor == review.actor,
                    "Credit reviewer does not match audit",
                )?;
                if review.approved {
                    self.ensure_credit_current(c, review.revision - 1)?;
                }
                events.push(review.revision);
            }
            if let Some(withdrawn) = c.withdrawn_revision {
                ensure(
                    withdrawn > c.submitted_revision.unwrap_or(c.created_revision)
                        && withdrawn <= self.revision,
                    "Credit withdrawal chronology invalid",
                )?;
                let audit = self.credit_audit(
                    c,
                    withdrawn,
                    "credit_withdraw",
                    &format!("Withdrew credit {}", c.proposal.id),
                )?;
                ensure(
                    audit.actor.name == c.owner
                        && matches!(audit.actor.role, Role::Analyst | Role::Admin),
                    "Only the credit owner may withdraw",
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
            ensure(valid, "Credit status/version/provenance mismatch")?;
            let audited: Vec<_> = self
                .audit
                .iter()
                .filter(|a| a.target.as_deref() == Some(&credit_target(&c.proposal.id)))
                .map(|a| a.revision)
                .collect();
            ensure(
                audited == events,
                "Credit history cannot be discarded or invented",
            )?;
        }
        for audit in &self.audit {
            if audit.command.starts_with("credit_")
                || audit
                    .target
                    .as_deref()
                    .is_some_and(|t| t.starts_with("credit:"))
            {
                let key = audit
                    .target
                    .as_deref()
                    .and_then(|t| t.strip_prefix("credit:"))
                    .ok_or_else(|| error("Credit audit target missing"))?;
                ensure(
                    self.credits.contains_key(key),
                    "Credit audit points to a missing record",
                )?;
            }
        }
        Ok(())
    }
}
