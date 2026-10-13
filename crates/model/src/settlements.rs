//! Local rebate reconciliation and independently reviewed settlement adjustments.
//! These records recognize a local obligation only; they never execute payments.
use super::*;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settlement {
    pub id: String,
    pub accrual_id: String,
    pub agreement_id: String,
    pub agreement_version: u64,
    pub currency: Currency,
    pub source_revision: u64,
    pub owner: String,
    pub reason: String,
    pub version: u64,
    pub status: RecordStatus,
    pub created_revision: u64,
    pub submitted_revision: Option<u64>,
    pub review: Option<RecordReview>,
    pub withdrawn_revision: Option<u64>,
    pub contributors: BTreeSet<String>,
    pub prior_settlement_ids: Vec<String>,
    pub previously_recognized: Money,
    pub target_rebate: Money,
    /// Signed true-up: full-period target minus prior approved local adjustments.
    pub adjustment: Money,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RebatePosition {
    pub agreement_id: String,
    pub currency: Currency,
    pub source_revision: u64,
    pub basis: AccrualBasis,
    pub rate: Bps,
    pub target_rebate: Money,
    pub approved_settlement_ids: Vec<String>,
    pub recognized_rebate: Money,
    pub remaining_adjustment: Money,
}

impl State {
    pub fn settlement_baseline(
        &self,
        agreement_id: &str,
        revision: u64,
    ) -> Result<(Vec<String>, Money)> {
        ensure(
            revision <= self.revision,
            "Settlement baseline revision is in the future",
        )?;
        let mut records: Vec<_> = self
            .settlements
            .values()
            .filter_map(|s| {
                s.review
                    .as_ref()
                    .filter(|r| {
                        s.agreement_id == agreement_id
                            && s.status == RecordStatus::Approved
                            && r.approved
                            && r.revision <= revision
                    })
                    .map(|r| (r.revision, s))
            })
            .collect();
        records.sort_by_key(|(revision, _)| *revision);
        let mut total = Money(0);
        let mut ids = Vec::with_capacity(records.len());
        for (_, record) in records {
            total = total.add(record.adjustment)?;
            ids.push(record.id.clone());
        }
        Ok((ids, total))
    }
    pub fn rebate_position(&self, agreement_id: &str, revision: u64) -> Result<RebatePosition> {
        let agreement = self
            .agreements
            .get(agreement_id)
            .ok_or_else(|| error("Agreement not found"))?;
        ensure(
            agreement.status == Status::Approved
                && agreement
                    .decisions
                    .last()
                    .is_some_and(|d| d.approved && d.revision <= revision),
            "Reconciliation requires an approved agreement at the source revision",
        )?;
        let basis = self.accrual_basis(agreement_id, revision)?;
        let rate = agreement
            .terms
            .tiers
            .iter()
            .filter(|t| t.threshold <= basis.eligible_invoice)
            .map(|t| t.rate)
            .next_back()
            .ok_or_else(|| error("No applicable rebate tier"))?;
        let target_rebate = basis.eligible_invoice.percent(rate)?;
        let (approved_settlement_ids, recognized_rebate) =
            self.settlement_baseline(agreement_id, revision)?;
        Ok(RebatePosition {
            agreement_id: agreement_id.into(),
            currency: agreement.terms.currency,
            source_revision: revision,
            basis,
            rate,
            target_rebate,
            approved_settlement_ids,
            recognized_rebate,
            remaining_adjustment: target_rebate.sub(recognized_rebate)?,
        })
    }
    pub fn settlement_contributors(
        &self,
        accrual: &Accrual,
        owner: &str,
    ) -> Result<BTreeSet<String>> {
        let mut names = BTreeSet::from([owner.to_owned()]);
        let agreement = self
            .agreements
            .get(&accrual.agreement_id)
            .ok_or_else(|| error("Settlement agreement missing"))?;
        names.insert(agreement.owner.clone());
        let accrual_revision = accrual
            .source_revision
            .checked_add(1)
            .ok_or_else(|| error("Accrual revision overflow"))?;
        names.insert(
            self.settlement_audit_at(accrual_revision)?
                .actor
                .name
                .clone(),
        );
        for id in &accrual.transaction_ids {
            if let Some(correction) = self.corrections.values().find(|c| {
                c.status == RecordStatus::Approved
                    && c.proposal.replacement.as_ref().is_some_and(|t| &t.id == id)
            }) {
                names.insert(correction.owner.clone());
            } else {
                let revision = *self
                    .transaction_revisions
                    .get(id)
                    .ok_or_else(|| error("Invoice provenance missing"))?;
                names.insert(self.settlement_audit_at(revision)?.actor.name.clone());
            }
        }
        for id in &accrual.correction_ids {
            let correction = self
                .corrections
                .get(id)
                .ok_or_else(|| error("Accrual correction missing"))?;
            names.insert(correction.owner.clone());
        }
        Ok(names)
    }
    /// Reject changed provenance even if its totals happen to be identical.
    pub fn ensure_settlement_current(&self, record: &Settlement, revision: u64) -> Result<()> {
        let accrual = self
            .accruals
            .get(&record.accrual_id)
            .ok_or_else(|| error("Settlement accrual missing"))?;
        let position = self.rebate_position(&record.agreement_id, revision)?;
        ensure(
            position.basis.transaction_ids == accrual.transaction_ids
                && position.basis.correction_ids == accrual.correction_ids
                && position.basis.eligible_invoice == accrual.eligible_invoice
                && position.rate == accrual.rate
                && position.target_rebate == record.target_rebate,
            "Settlement source is stale; calculate a new accrual and create a new proposal",
        )?;
        ensure(
            position.approved_settlement_ids == record.prior_settlement_ids
                && position.recognized_rebate == record.previously_recognized,
            "Settlement baseline changed; create a new proposal against current approved adjustments",
        )
    }
    fn settlement_audit_at(&self, revision: u64) -> Result<&Audit> {
        self.audit
            .iter()
            .find(|a| a.revision == revision)
            .ok_or_else(|| error("Settlement provenance audit missing"))
    }
    fn settlement_event(
        &self,
        record: &Settlement,
        revision: u64,
        command: &str,
        detail: &str,
        actor: &str,
        finance: bool,
    ) -> Result<()> {
        let audit = self.settlement_audit_at(revision)?;
        ensure(
            audit.target.as_deref() == Some(format!("settlement:{}", record.id).as_str())
                && audit.command == command
                && audit.detail == detail
                && audit.actor.name == actor
                && if finance {
                    audit.actor.role == Role::Finance
                } else {
                    matches!(audit.actor.role, Role::Analyst | Role::Admin)
                },
            "Settlement event does not match its exact audit evidence",
        )
    }
    pub(crate) fn validate_settlements(&self) -> Result<()> {
        ensure(
            self.settlements.len() <= 1000,
            "Settlement record limit reached",
        )?;
        for (key, record) in &self.settlements {
            id(&record.id)?;
            text(&record.owner)?;
            text(&record.reason)?;
            ensure(
                key == &record.id
                    && record.created_revision > 0
                    && record.created_revision <= self.revision
                    && record.source_revision.checked_add(1) == Some(record.created_revision),
                "Invalid settlement identity or chronology",
            )?;
            let accrual = self
                .accruals
                .get(&record.accrual_id)
                .ok_or_else(|| error("Settlement accrual missing"))?;
            ensure(
                accrual.source_revision < record.source_revision
                    && accrual.agreement_id == record.agreement_id
                    && accrual.agreement_version == record.agreement_version
                    && accrual.currency == record.currency
                    && accrual.rebate == record.target_rebate,
                "Settlement does not match its frozen accrual",
            )?;
            ensure(
                record.contributors == self.settlement_contributors(accrual, &record.owner)?,
                "Settlement contributor exclusions were altered",
            )?;
            ensure(
                record.target_rebate.0 >= 0
                    && record.previously_recognized.0 >= 0
                    && record.adjustment
                        == record.target_rebate.sub(record.previously_recognized)?
                    && record.adjustment != Money(0),
                "Invalid settlement true-up arithmetic",
            )?;
            self.ensure_settlement_current(record, record.source_revision)?;
            self.settlement_event(
                record,
                record.created_revision,
                "settlement_create",
                &settlement_created_detail(record)?,
                &record.owner,
                false,
            )?;
            let mut expected = vec![(record.created_revision, "settlement_create")];
            if let Some(revision) = record.submitted_revision {
                ensure(
                    revision > record.created_revision && revision <= self.revision,
                    "Settlement submission chronology invalid",
                )?;
                self.ensure_settlement_current(
                    record,
                    revision
                        .checked_sub(1)
                        .ok_or_else(|| error("Invalid submission revision"))?,
                )?;
                self.settlement_event(
                    record,
                    revision,
                    "settlement_submit",
                    &format!("Submitted settlement {}", record.id),
                    &record.owner,
                    false,
                )?;
                expected.push((revision, "settlement_submit"));
            }
            if let Some(review) = &record.review {
                text(&review.note)?;
                text(&review.actor.name)?;
                ensure(
                    record
                        .submitted_revision
                        .is_some_and(|r| review.revision > r)
                        && review.revision <= self.revision
                        && review.actor.role == Role::Finance
                        && !record.contributors.contains(&review.actor.name),
                    "Settlement requires independent Finance review after submission",
                )?;
                self.settlement_event(
                    record,
                    review.revision,
                    "settlement_decide",
                    &settlement_decided_detail(record, review.approved)?,
                    &review.actor.name,
                    true,
                )?;
                let event = self.settlement_audit_at(review.revision)?;
                ensure(
                    event.actor == review.actor,
                    "Settlement review actor differs from audit",
                )?;
                if review.approved {
                    self.ensure_settlement_current(
                        record,
                        review
                            .revision
                            .checked_sub(1)
                            .ok_or_else(|| error("Invalid review revision"))?,
                    )?;
                }
                expected.push((review.revision, "settlement_decide"));
            }
            if let Some(revision) = record.withdrawn_revision {
                ensure(
                    revision > record.submitted_revision.unwrap_or(record.created_revision)
                        && revision <= self.revision,
                    "Settlement withdrawal chronology invalid",
                )?;
                self.settlement_event(
                    record,
                    revision,
                    "settlement_withdraw",
                    &format!("Withdrew settlement {}", record.id),
                    &record.owner,
                    false,
                )?;
                expected.push((revision, "settlement_withdraw"));
            }
            let valid = match record.status {
                RecordStatus::Draft => {
                    record.version == 1
                        && record.submitted_revision.is_none()
                        && record.review.is_none()
                        && record.withdrawn_revision.is_none()
                }
                RecordStatus::Pending => {
                    record.version == 2
                        && record.submitted_revision.is_some()
                        && record.review.is_none()
                        && record.withdrawn_revision.is_none()
                }
                RecordStatus::Approved | RecordStatus::Rejected => {
                    record.version == 3
                        && record.submitted_revision.is_some()
                        && record.review.as_ref().is_some_and(|r| {
                            r.approved == (record.status == RecordStatus::Approved)
                        })
                        && record.withdrawn_revision.is_none()
                }
                RecordStatus::Withdrawn => {
                    record.version
                        == (if record.submitted_revision.is_some() {
                            3
                        } else {
                            2
                        })
                        && record.review.is_none()
                        && record.withdrawn_revision.is_some()
                }
            };
            ensure(valid, "Settlement lifecycle or version invalid")?;
            let target = format!("settlement:{}", record.id);
            let actual: Vec<_> = self
                .audit
                .iter()
                .filter(|a| a.target.as_deref() == Some(&target))
                .map(|a| (a.revision, a.command.as_str()))
                .collect();
            expected.sort_by_key(|(r, _)| *r);
            ensure(
                actual == expected,
                "Settlement events were erased or detached",
            )?;
        }
        for audit in self.audit.iter().filter(|a| {
            a.command.starts_with("settlement_")
                || a.target
                    .as_deref()
                    .is_some_and(|t| t.starts_with("settlement:"))
        }) {
            let key = audit
                .target
                .as_deref()
                .and_then(|t| t.strip_prefix("settlement:"))
                .ok_or_else(|| error("Settlement audit target missing"))?;
            ensure(
                self.settlements.contains_key(key),
                "Settlement record was removed",
            )?;
        }
        Ok(())
    }
}
pub fn settlement_created_detail(record: &Settlement) -> Result<String> {
    let fingerprint = record_fingerprint(&(
        &record.id,
        &record.accrual_id,
        &record.agreement_id,
        record.agreement_version,
        record.currency,
        record.source_revision,
        &record.owner,
        &record.reason,
        record.created_revision,
        &record.contributors,
        &record.prior_settlement_ids,
        record.previously_recognized,
        record.target_rebate,
        record.adjustment,
    ))?;
    Ok(format!("Created settlement {}: {fingerprint}", record.id))
}
pub fn settlement_decided_detail(record: &Settlement, approved: bool) -> Result<String> {
    let fingerprint = record_fingerprint(&(settlement_created_detail(record)?, &record.review))?;
    Ok(format!(
        "{} settlement {}: {fingerprint}",
        if approved { "Approved" } else { "Rejected" },
        record.id
    ))
}
