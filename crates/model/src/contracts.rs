//! Immutable customer price-agreement editions, interval selection and provenance.
use super::*;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ContractBasis {
    Fixed { invoice_unit: Money },
    ApprovedSource { request: PriceRequest },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContractRow {
    pub id: String,
    pub product_id: String,
    pub unit: String,
    pub min_quantity: Quantity,
    pub max_quantity: Option<Quantity>,
    pub basis: ContractBasis,
}
impl ContractRow {
    pub fn contains(&self, quantity: Quantity) -> bool {
        quantity >= self.min_quantity && self.max_quantity.is_none_or(|max| quantity < max)
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContractTerms {
    pub id: String,
    pub edition: u64,
    pub name: String,
    pub customer_id: String,
    pub currency: Currency,
    pub start: Date,
    pub end: Date,
    pub priority: i32,
    pub rows: Vec<ContractRow>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContractRowEvidence {
    pub row_id: String,
    pub invoice_unit: Money,
    pub origin: Option<SourceEvidence>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PriceContract {
    pub terms: ContractTerms,
    pub rows: Vec<ContractRowEvidence>,
    pub owner: String,
    pub contributors: BTreeSet<String>,
    pub created_revision: u64,
    pub submitted_revision: Option<u64>,
    pub review: Option<RecordReview>,
    pub withdrawn_revision: Option<u64>,
    /// Workflow version; separate from immutable business edition.
    pub version: u64,
    pub status: RecordStatus,
}
impl PriceContract {
    pub fn approved_at(&self, revision: u64) -> bool {
        self.status == RecordStatus::Approved
            && self
                .review
                .as_ref()
                .is_some_and(|r| r.approved && r.revision <= revision)
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContractSourceEvidence {
    pub row_id: String,
    pub start: Date,
    pub end: Date,
    pub max_quantity: Option<Quantity>,
    pub priority: i32,
    pub terms_fingerprint: String,
}
pub fn contract_key(id: &str, edition: u64) -> String {
    format!("{id}:{edition}")
}
pub fn contract_target(id: &str, edition: u64) -> String {
    format!("contract:{id}:{edition}")
}
pub fn contract_fingerprint(record: &PriceContract) -> Result<String> {
    record_fingerprint(&(
        &record.terms,
        &record.rows,
        &record.owner,
        &record.contributors,
        record.created_revision,
    ))
}
pub fn contract_created_detail(record: &PriceContract) -> Result<String> {
    Ok(format!(
        "Created contract {}: {}",
        record.terms.id,
        contract_fingerprint(record)?
    ))
}
pub fn contract_decided_detail(record: &PriceContract) -> Result<String> {
    Ok(format!(
        "Reviewed contract {}: {}",
        record.terms.id,
        record_fingerprint(&(contract_fingerprint(record)?, &record.review))?
    ))
}
fn bands_overlap(a: &ContractRow, b: &ContractRow) -> bool {
    a.max_quantity.is_none_or(|end| b.min_quantity < end)
        && b.max_quantity.is_none_or(|end| a.min_quantity < end)
}
impl State {
    pub fn contract(&self, id: &str, edition: u64) -> Result<&PriceContract> {
        self.price_contracts
            .get(&contract_key(id, edition))
            .ok_or_else(|| error("Price contract edition not found"))
    }
    pub fn latest_contract_edition(&self, id: &str) -> u64 {
        self.price_contracts
            .values()
            .filter(|c| c.terms.id == id)
            .map(|c| c.terms.edition)
            .max()
            .unwrap_or(0)
    }
    fn validate_contract_terms(&self, terms: &ContractTerms) -> Result<()> {
        id(&terms.id)?;
        text(&terms.name)?;
        id(&terms.customer_id)?;
        terms.start.validate()?;
        terms.end.validate()?;
        ensure(
            terms.edition > 0 && terms.start < terms.end,
            "Contract edition must be positive; end is exclusive and follows start",
        )?;
        ensure(
            self.customers.contains_key(&terms.customer_id),
            "Contract customer not found",
        )?;
        ensure(
            !terms.rows.is_empty() && terms.rows.len() <= 100,
            "Contract requires 1–100 price rows",
        )?;
        ensure(
            (-10_000..=10_000).contains(&terms.priority),
            "Contract priority must be between -10000 and 10000",
        )?;
        let mut ids = BTreeSet::new();
        for row in &terms.rows {
            id(&row.id)?;
            id(&row.product_id)?;
            id(&row.unit)?;
            ensure(ids.insert(&row.id), "Duplicate contract row ID")?;
            row.min_quantity.validate()?;
            if let Some(max) = row.max_quantity {
                max.validate()?;
                ensure(
                    max > row.min_quantity,
                    "Quantity band end is exclusive and must exceed minimum",
                )?;
            }
            let product = self
                .products
                .get(&row.product_id)
                .ok_or_else(|| error("Contract product not found"))?;
            ensure(
                product.currency == terms.currency && product.unit == row.unit,
                "Contract currency/UOM mismatch; no implicit conversion",
            )?;
        }
        for (index, a) in terms.rows.iter().enumerate() {
            for b in terms.rows.iter().skip(index + 1) {
                ensure(
                    a.product_id != b.product_id || !bands_overlap(a, b),
                    "Overlapping quantity bands within a contract edition",
                )?;
            }
        }
        Ok(())
    }
    pub fn freeze_contract_rows(
        &self,
        terms: &ContractTerms,
        revision: u64,
        historical: bool,
    ) -> Result<Vec<ContractRowEvidence>> {
        self.validate_contract_terms(terms)?;
        terms.rows.iter().map(|row| {
            let (invoice_unit, origin) = match &row.basis {
                ContractBasis::Fixed { invoice_unit } => { invoice_unit.nonnegative()?; (*invoice_unit, None) }
                ContractBasis::ApprovedSource { request } => {
                    ensure(matches!(request.source, Some(PriceSource::Grid { .. } | PriceSource::PriceList { .. })),
                        "Contract origins must be explicit grid/list snapshots; contract chaining is unsupported")?;
                    ensure(request.product_id == row.product_id && request.customer_id == terms.customer_id
                        && request.discount == Bps(0) && request.rebate == Bps(0),
                        "Origin product/customer must match and origin capture cannot add discounts or rebates")?;
                    let evidence = self.pricing_source_at(request, revision, historical)?
                        .ok_or_else(|| error("Approved origin required"))?;
                    ensure(evidence.currency == terms.currency && evidence.unit == row.unit
                        && row.min_quantity >= evidence.min_quantity,
                        "Contract cannot change origin currency/UOM or undercut its minimum quantity")?;
                    (evidence.invoice_unit, Some(evidence))
                }
            };
            Ok(ContractRowEvidence { row_id: row.id.clone(), invoice_unit, origin })
        }).collect()
    }
    pub fn contract_contributors(
        &self,
        rows: &[ContractRowEvidence],
        owner: &str,
    ) -> Result<BTreeSet<String>> {
        let mut names = BTreeSet::from([owner.to_owned()]);
        for origin in rows.iter().filter_map(|r| r.origin.as_ref()) {
            let revision = origin
                .pricing_revision
                .checked_add(1)
                .ok_or_else(|| error("Origin revision overflow"))?;
            let audit = self
                .audit
                .iter()
                .find(|a| a.revision == revision)
                .ok_or_else(|| error("Origin creation audit missing"))?;
            let (command, target) = match &origin.source {
                PriceSource::Grid { id, .. } => ("grid_calculate", format!("grid:{id}")),
                PriceSource::PriceList { id, .. } => ("list_calculate", format!("list:{id}")),
                _ => return Err(error("Invalid contract origin")),
            };
            ensure(
                audit.command == command
                    && audit.target.as_deref().is_none_or(|t| t == target)
                    && matches!(audit.actor.role, Role::Analyst | Role::Admin),
                "Origin author evidence mismatch",
            )?;
            names.insert(audit.actor.name.clone());
        }
        Ok(names)
    }
    /// Effective interval after successor editions, as known at a specific revision.
    pub fn contract_effective_end(&self, record: &PriceContract, revision: u64) -> Date {
        self.price_contracts
            .values()
            .filter(|c| {
                c.terms.id == record.terms.id
                    && c.terms.edition > record.terms.edition
                    && c.approved_at(revision)
            })
            .map(|c| &c.terms.start)
            .chain(std::iter::once(&record.terms.end))
            .min()
            .cloned()
            .unwrap_or_else(|| record.terms.end.clone())
    }
    /// Approval cannot introduce ambiguous equal-priority commercial coverage.
    pub fn ensure_contract_approval(&self, record: &PriceContract, revision: u64) -> Result<()> {
        for other in self
            .price_contracts
            .values()
            .filter(|c| c.approved_at(revision))
        {
            if other.terms.id == record.terms.id {
                if other.terms.edition == record.terms.edition {
                    continue;
                }
                ensure(
                    record.terms.edition > other.terms.edition
                        && record.terms.start >= other.terms.start,
                    "Approve increasing editions with nondecreasing start dates; older pending editions are stale",
                )?;
                continue;
            }
            if other.terms.customer_id != record.terms.customer_id
                || other.terms.currency != record.terms.currency
                || other.terms.priority != record.terms.priority
            {
                continue;
            }
            let end = self.contract_effective_end(other, revision);
            if other.terms.start >= end
                || record.terms.start >= end
                || other.terms.start >= record.terms.end
            {
                continue;
            }
            for a in &record.terms.rows {
                for b in &other.terms.rows {
                    ensure(
                        a.product_id != b.product_id || !bands_overlap(a, b),
                        "Equal-priority contracts overlap in date/product/quantity; change the interval, band or explicit priority",
                    )?;
                }
            }
        }
        Ok(())
    }
    fn active_contract(&self, id: &str, date: &Date, revision: u64) -> Option<&PriceContract> {
        self.price_contracts
            .values()
            .filter(|c| c.terms.id == id && c.approved_at(revision) && &c.terms.start <= date)
            .max_by_key(|c| c.terms.edition)
    }
    pub(crate) fn contract_source_evidence(
        &self,
        request: &PriceRequest,
        revision: u64,
    ) -> Result<SourceEvidence> {
        ensure(
            revision <= self.revision,
            "Contract source revision is in the future",
        )?;
        let qualifies = |c: &PriceContract| {
            c.terms.customer_id == request.customer_id
                && c.terms.start <= request.date
                && request.date < c.terms.end
                && c.terms
                    .rows
                    .iter()
                    .any(|r| r.product_id == request.product_id && r.contains(request.quantity))
        };
        let selected = match request.source.as_ref() {
            Some(PriceSource::Contract { id, edition }) => {
                let c = self
                    .active_contract(id, &request.date, revision)
                    .ok_or_else(|| error("No approved contract edition has started"))?;
                ensure(
                    c.terms.edition == *edition,
                    "Selected contract edition is superseded; select the current edition deliberately",
                )?;
                ensure(
                    qualifies(c),
                    "Contract is outside its date/product/customer/quantity eligibility",
                )?;
                c
            }
            Some(PriceSource::CustomerContract) => {
                let families: BTreeSet<_> = self
                    .price_contracts
                    .values()
                    .map(|c| c.terms.id.as_str())
                    .collect();
                let mut candidates: Vec<_> = families
                    .iter()
                    .filter_map(|id| self.active_contract(id, &request.date, revision))
                    .filter(|c| qualifies(c))
                    .collect();
                candidates.sort_by_key(|c| std::cmp::Reverse(c.terms.priority));
                let first = candidates.first().copied().ok_or_else(|| {
                    error("No eligible approved customer contract; no live-price fallback")
                })?;
                ensure(
                    candidates
                        .get(1)
                        .is_none_or(|c| c.terms.priority != first.terms.priority),
                    "Ambiguous contract priority",
                )?;
                first
            }
            _ => return Err(error("A contract selector is required")),
        };
        let row = selected
            .terms
            .rows
            .iter()
            .find(|r| r.product_id == request.product_id && r.contains(request.quantity))
            .ok_or_else(|| error("Contract price row missing"))?;
        let frozen = selected
            .rows
            .iter()
            .find(|r| r.row_id == row.id)
            .ok_or_else(|| error("Contract frozen row missing"))?;
        let review = selected
            .review
            .as_ref()
            .ok_or_else(|| error("Contract approval missing"))?;
        Ok(SourceEvidence {
            source: PriceSource::Contract {
                id: selected.terms.id.clone(),
                edition: selected.terms.edition,
            },
            document_version: Some(selected.terms.edition),
            pricing_revision: selected.created_revision,
            approval_revision: review.revision,
            publication_revision: None,
            captured_revision: revision,
            product_id: row.product_id.clone(),
            customer_id: selected.terms.customer_id.clone(),
            currency: selected.terms.currency,
            unit: row.unit.clone(),
            effective_date: request.date.clone(),
            min_quantity: row.min_quantity,
            invoice_unit: frozen.invoice_unit,
            contract: Some(ContractSourceEvidence {
                row_id: row.id.clone(),
                start: selected.terms.start.clone(),
                end: self.contract_effective_end(selected, revision),
                max_quantity: row.max_quantity,
                priority: selected.terms.priority,
                terms_fingerprint: contract_fingerprint(selected)?,
            }),
        })
    }
    fn contract_event(
        &self,
        c: &PriceContract,
        revision: u64,
        command: &str,
        detail: &str,
        actor: &str,
        review: bool,
    ) -> Result<()> {
        let audit = self
            .audit
            .iter()
            .find(|a| a.revision == revision)
            .ok_or_else(|| error("Contract audit missing"))?;
        ensure(
            audit.target.as_deref() == Some(contract_target(&c.terms.id, c.terms.edition).as_str())
                && audit.command == command
                && audit.detail == detail
                && audit.actor.name == actor
                && if review {
                    audit.actor.role == Role::Approver
                } else {
                    matches!(audit.actor.role, Role::Analyst | Role::Admin)
                },
            "Contract event differs from exact audit evidence",
        )
    }
    pub(crate) fn validate_price_contracts(&self) -> Result<()> {
        ensure(
            self.price_contracts.len() <= 500
                && self
                    .price_contracts
                    .values()
                    .map(|c| c.terms.rows.len())
                    .sum::<usize>()
                    <= 5000,
            "Price contract record/row limit reached",
        )?;
        for (key, c) in &self.price_contracts {
            self.validate_contract_terms(&c.terms)?;
            text(&c.owner)?;
            ensure(
                key == &contract_key(&c.terms.id, c.terms.edition)
                    && c.created_revision > 0
                    && c.created_revision <= self.revision,
                "Contract identity/creation invalid",
            )?;
            let previous: Vec<_> = self
                .price_contracts
                .values()
                .filter(|old| old.terms.id == c.terms.id && old.terms.edition < c.terms.edition)
                .collect();
            ensure(
                previous.len() as u64 == c.terms.edition - 1
                    && previous.iter().all(|old| {
                        old.created_revision < c.created_revision
                            && old.owner == c.owner
                            && old.terms.customer_id == c.terms.customer_id
                            && old.terms.currency == c.terms.currency
                    }),
                "Contract editions must be contiguous, chronological and retain owner/customer/currency",
            )?;
            let frozen = self.freeze_contract_rows(&c.terms, c.created_revision - 1, true)?;
            ensure(
                c.rows == frozen
                    && c.contributors == self.contract_contributors(&frozen, &c.owner)?,
                "Contract frozen rows or contributor evidence changed",
            )?;
            self.contract_event(
                c,
                c.created_revision,
                "contract_create",
                &contract_created_detail(c)?,
                &c.owner,
                false,
            )?;
            let mut expected = vec![(c.created_revision, "contract_create")];
            if let Some(r) = c.submitted_revision {
                ensure(
                    r > c.created_revision && r <= self.revision,
                    "Contract submission chronology invalid",
                )?;
                self.contract_event(
                    c,
                    r,
                    "contract_submit",
                    &format!("Submitted contract {}", c.terms.id),
                    &c.owner,
                    false,
                )?;
                expected.push((r, "contract_submit"));
            }
            if let Some(r) = &c.review {
                text(&r.note)?;
                text(&r.actor.name)?;
                ensure(
                    c.submitted_revision.is_some_and(|s| r.revision > s)
                        && r.revision <= self.revision
                        && r.actor.role == Role::Approver
                        && !c.contributors.contains(&r.actor.name),
                    "Contract requires independent commercial review",
                )?;
                self.contract_event(
                    c,
                    r.revision,
                    "contract_decide",
                    &contract_decided_detail(c)?,
                    &r.actor.name,
                    true,
                )?;
                if r.approved {
                    self.ensure_contract_approval(c, r.revision - 1)?;
                }
                expected.push((r.revision, "contract_decide"));
            }
            if let Some(r) = c.withdrawn_revision {
                ensure(
                    r > c.submitted_revision.unwrap_or(c.created_revision) && r <= self.revision,
                    "Contract withdrawal chronology invalid",
                )?;
                self.contract_event(
                    c,
                    r,
                    "contract_withdraw",
                    &format!("Withdrew contract {}", c.terms.id),
                    &c.owner,
                    false,
                )?;
                expected.push((r, "contract_withdraw"));
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
                        && c.review
                            .as_ref()
                            .is_some_and(|r| r.approved == (c.status == RecordStatus::Approved))
                        && c.withdrawn_revision.is_none()
                }
                RecordStatus::Withdrawn => {
                    c.version == if c.submitted_revision.is_some() { 3 } else { 2 }
                        && c.review.is_none()
                        && c.withdrawn_revision.is_some()
                }
            };
            ensure(valid, "Contract lifecycle/version invalid")?;
            expected.sort_by_key(|x| x.0);
            let target = contract_target(&c.terms.id, c.terms.edition);
            let actual: Vec<_> = self
                .audit
                .iter()
                .filter(|a| a.target.as_deref() == Some(&target))
                .map(|a| (a.revision, a.command.as_str()))
                .collect();
            ensure(
                actual == expected,
                "Contract events were detached or erased",
            )?;
        }
        for a in self.audit.iter().filter(|a| {
            a.command.starts_with("contract_")
                || a.target
                    .as_deref()
                    .is_some_and(|t| t.starts_with("contract:"))
        }) {
            let key = a
                .target
                .as_deref()
                .and_then(|t| t.strip_prefix("contract:"))
                .ok_or_else(|| error("Contract audit target missing"))?;
            ensure(
                self.price_contracts.contains_key(key),
                "Contract record was removed",
            )?;
        }
        Ok(())
    }
}
