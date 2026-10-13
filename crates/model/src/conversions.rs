//! Explicit direct conversion schedules and frozen native pricing evidence.
use super::*;
use copperline_money::{Rational, convert_unit};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ConversionKind {
    Fx {
        from: Currency,
        to: Currency,
    },
    /// Native product units per requested unit. No inferred inverse or unit graph.
    Uom {
        product_id: String,
        from_unit: String,
        to_unit: String,
    },
}
impl ConversionKind {
    pub fn label(&self) -> String {
        match self {
            Self::Fx { from, to } => format!("FX {from} to {to}"),
            Self::Uom {
                product_id,
                from_unit,
                to_unit,
            } => format!("UOM {product_id}: {from_unit} to {to_unit}"),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConversionEntry {
    pub id: String,
    pub conversion: ConversionKind,
    pub start: Date,
    pub end: Date,
    pub ratio: Rational,
}
impl ConversionEntry {
    pub fn validate(&self) -> Result<()> {
        id(&self.id)?;
        self.start.validate()?;
        self.end.validate()?;
        self.ratio.validate()?;
        ensure(
            self.start < self.end,
            "Conversion requires a nonempty half-open date interval",
        )?;
        match &self.conversion {
            ConversionKind::Fx { from, to } => {
                ensure(from != to, "Identity currency requires no conversion rate")
            }
            ConversionKind::Uom {
                product_id,
                from_unit,
                to_unit,
            } => {
                id(product_id)?;
                id(from_unit)?;
                id(to_unit)?;
                ensure(
                    from_unit != to_unit,
                    "Identity unit requires no conversion factor",
                )
            }
        }
    }
    fn overlaps(&self, other: &Self) -> bool {
        self.conversion == other.conversion && self.start < other.end && other.start < self.end
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConversionTerms {
    pub id: String,
    pub name: String,
    pub entries: Vec<ConversionEntry>,
}
impl ConversionTerms {
    pub fn validate(&self) -> Result<()> {
        id(&self.id)?;
        text(&self.name)?;
        ensure(
            !self.entries.is_empty() && self.entries.len() <= 100,
            "Conversion book requires 1–100 rows",
        )?;
        let mut ids = BTreeSet::new();
        for (i, row) in self.entries.iter().enumerate() {
            row.validate()?;
            ensure(ids.insert(&row.id), "Duplicate conversion row ID")?;
            ensure(
                !self.entries[..i].iter().any(|old| old.overlaps(row)),
                "Conversion rows overlap for the same direct key",
            )?;
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConversionBook {
    pub terms: ConversionTerms,
    pub owner: String,
    pub created_revision: u64,
    pub submitted_revision: Option<u64>,
    pub review: Option<RecordReview>,
    pub withdrawn_revision: Option<u64>,
    pub version: u64,
    pub status: RecordStatus,
}
impl ConversionBook {
    pub fn approved_at(&self, revision: u64) -> bool {
        self.status == RecordStatus::Approved
            && self
                .review
                .as_ref()
                .is_some_and(|r| r.approved && r.revision <= revision)
    }
}
pub fn conversion_target(key: &str) -> String {
    format!("conversion:{key}")
}
pub fn conversion_fingerprint(book: &ConversionBook) -> Result<String> {
    record_fingerprint(&(&book.terms, &book.owner, book.created_revision))
}
pub fn conversion_created_detail(book: &ConversionBook) -> Result<String> {
    Ok(format!(
        "Created conversion {}: {}",
        book.terms.id,
        conversion_fingerprint(book)?
    ))
}
pub fn conversion_decided_detail(book: &ConversionBook) -> Result<String> {
    Ok(format!(
        "Reviewed conversion {}: {}",
        book.terms.id,
        record_fingerprint(&(conversion_fingerprint(book)?, &book.review))?
    ))
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RateEvidence {
    pub book_id: String,
    pub book_version: u64,
    pub entry: ConversionEntry,
    pub approval_revision: u64,
    pub captured_revision: u64,
    pub terms_fingerprint: String,
}
impl RateEvidence {
    fn validate_for(&self, key: &ConversionKind, date: &Date, revision: u64) -> Result<()> {
        id(&self.book_id)?;
        self.entry.validate()?;
        ensure(
            &self.entry.conversion == key && self.entry.start <= *date && *date < self.entry.end,
            "Frozen conversion direction or effective date mismatch",
        )?;
        ensure(
            self.book_version == 3
                && self.approval_revision > 0
                && self.approval_revision <= self.captured_revision
                && self.captured_revision == revision
                && self.terms_fingerprint.len() == 64
                && self
                    .terms_fingerprint
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit()),
            "Frozen conversion version/revision/fingerprint invalid",
        )
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConversionRequest {
    pub currency: Currency,
    pub unit: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversionRounding {
    NativeUnitCombinedHalfAwayThenExtendV1,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConversionEvidence {
    pub native_currency: Currency,
    pub native_unit: String,
    pub native_quantity: Quantity,
    pub native_list_unit: Money,
    pub native_rule_unit: Money,
    pub native_cost_unit: Money,
    pub fx: Option<RateEvidence>,
    pub uom: Option<RateEvidence>,
    pub captured_revision: u64,
    pub rounding: ConversionRounding,
}
impl ConversionEvidence {
    pub fn convert_unit(&self, amount: Money) -> Result<Money> {
        convert_unit(
            amount,
            self.uom
                .as_ref()
                .map(|r| r.entry.ratio)
                .unwrap_or(Rational::ONE),
            self.fx
                .as_ref()
                .map(|r| r.entry.ratio)
                .unwrap_or(Rational::ONE),
        )
    }
    pub fn validate_for(&self, request: &PriceRequest) -> Result<()> {
        let selection = request
            .conversion
            .as_ref()
            .ok_or_else(|| error("Conversion evidence requires explicit request"))?;
        id(&selection.unit)?;
        id(&self.native_unit)?;
        ensure(
            selection.currency != self.native_currency || selection.unit != self.native_unit,
            "Identity request must omit conversion; do not convert a converted value again",
        )?;
        match (&self.fx, selection.currency == self.native_currency) {
            (None, true) => {}
            (Some(rate), false) => rate.validate_for(
                &ConversionKind::Fx {
                    from: self.native_currency,
                    to: selection.currency,
                },
                &request.date,
                self.captured_revision,
            )?,
            _ => return Err(error("Explicit direct FX evidence missing or redundant")),
        }
        match (&self.uom, selection.unit == self.native_unit) {
            (None, true) => {}
            (Some(rate), false) => rate.validate_for(
                &ConversionKind::Uom {
                    product_id: request.product_id.clone(),
                    from_unit: selection.unit.clone(),
                    to_unit: self.native_unit.clone(),
                },
                &request.date,
                self.captured_revision,
            )?,
            _ => return Err(error("Explicit direct UOM evidence missing or redundant")),
        }
        let normalized = self
            .uom
            .as_ref()
            .map(|r| r.entry.ratio)
            .unwrap_or(Rational::ONE)
            .quantity(request.quantity)?;
        ensure(
            normalized == self.native_quantity,
            "Frozen native quantity does not match exact declared factor",
        )?;
        for m in [
            self.native_list_unit,
            self.native_rule_unit,
            self.native_cost_unit,
        ] {
            ensure(m.0 >= 0, "Native unit amount must be nonnegative")?;
        }
        Ok(())
    }
}
impl Evaluation {
    pub fn native_request(&self) -> PriceRequest {
        let mut request = self.request.clone();
        if let Some(c) = &self.conversion {
            request.quantity = c.native_quantity;
            request.conversion = None;
        }
        request
    }
    pub(crate) fn native_source_context(&self) -> Self {
        let mut native = self.clone();
        if let Some(c) = &self.conversion {
            native.request = self.native_request();
            native.currency = c.native_currency;
            native.unit = c.native_unit.clone();
            native.list_unit = c.native_list_unit;
            native.rule_unit = c.native_rule_unit;
            native.cost_unit = c.native_cost_unit;
            native.conversion = None;
        }
        native
    }
    pub(crate) fn validate_conversion_snapshot(&self) -> Result<()> {
        match (&self.request.conversion, &self.conversion) {
            (None, None) => Ok(()),
            (Some(selection), Some(c)) => {
                c.validate_for(&self.request)?;
                ensure(
                    self.currency == selection.currency
                        && self.unit == selection.unit
                        && self.list_unit == c.convert_unit(c.native_list_unit)?
                        && self.rule_unit == c.convert_unit(c.native_rule_unit)?
                        && self.cost_unit == c.convert_unit(c.native_cost_unit)?,
                    "Converted unit rounding or output context mismatch",
                )
            }
            _ => Err(error(
                "Conversion request and frozen evidence must occur together",
            )),
        }
    }
}
impl State {
    pub fn validate_conversion_terms(&self, terms: &ConversionTerms) -> Result<()> {
        terms.validate()?;
        for row in &terms.entries {
            if let ConversionKind::Uom {
                product_id,
                to_unit,
                ..
            } = &row.conversion
            {
                let p = self
                    .products
                    .get(product_id)
                    .ok_or_else(|| error("UOM product not found"))?;
                ensure(
                    &p.unit == to_unit,
                    "UOM rows must map requested units directly to this product's immutable native unit",
                )?;
            }
        }
        Ok(())
    }
    pub fn ensure_conversion_approval(&self, book: &ConversionBook, revision: u64) -> Result<()> {
        for other in self
            .conversion_books
            .values()
            .filter(|b| b.terms.id != book.terms.id && b.approved_at(revision))
        {
            ensure(
                !book
                    .terms
                    .entries
                    .iter()
                    .any(|a| other.terms.entries.iter().any(|b| a.overlaps(b))),
                "Approved conversion already covers this direct key/date; overlapping amendments are unsupported",
            )?;
        }
        Ok(())
    }
    pub fn conversion_rate_at(
        &self,
        key: &ConversionKind,
        date: &Date,
        revision: u64,
    ) -> Result<RateEvidence> {
        date.validate()?;
        ensure(
            revision <= self.revision,
            "Conversion source revision is in the future",
        )?;
        let mut found = None;
        for book in self
            .conversion_books
            .values()
            .filter(|b| b.approved_at(revision))
        {
            for row in book
                .terms
                .entries
                .iter()
                .filter(|r| &r.conversion == key && r.start <= *date && *date < r.end)
            {
                ensure(found.is_none(), "Ambiguous approved conversion coverage")?;
                let review = book
                    .review
                    .as_ref()
                    .ok_or_else(|| error("Conversion approval missing"))?;
                found = Some(RateEvidence {
                    book_id: book.terms.id.clone(),
                    book_version: book.version,
                    entry: row.clone(),
                    approval_revision: review.revision,
                    captured_revision: revision,
                    terms_fingerprint: conversion_fingerprint(book)?,
                });
            }
        }
        found.ok_or_else(|| {
            error(format!(
                "No approved direct {} on {date}; no inverse or multihop conversion is inferred",
                key.label()
            ))
        })
    }
    pub fn prepare_conversion(&self, request: &PriceRequest) -> Result<Option<ConversionEvidence>> {
        let Some(selection) = &request.conversion else {
            return Ok(None);
        };
        id(&selection.unit)?;
        let p = self
            .products
            .get(&request.product_id)
            .ok_or_else(|| error("Product not found"))?;
        let fx = if p.currency == selection.currency {
            None
        } else {
            Some(self.conversion_rate_at(
                &ConversionKind::Fx {
                    from: p.currency,
                    to: selection.currency,
                },
                &request.date,
                self.revision,
            )?)
        };
        let uom = if p.unit == selection.unit {
            None
        } else {
            Some(self.conversion_rate_at(
                &ConversionKind::Uom {
                    product_id: p.id.clone(),
                    from_unit: selection.unit.clone(),
                    to_unit: p.unit.clone(),
                },
                &request.date,
                self.revision,
            )?)
        };
        let c = ConversionEvidence {
            native_currency: p.currency,
            native_unit: p.unit.clone(),
            native_quantity: uom
                .as_ref()
                .map(|r| r.entry.ratio)
                .unwrap_or(Rational::ONE)
                .quantity(request.quantity)?,
            native_list_unit: Money(0),
            native_rule_unit: Money(0),
            native_cost_unit: Money(0),
            fx,
            uom,
            captured_revision: self.revision,
            rounding: ConversionRounding::NativeUnitCombinedHalfAwayThenExtendV1,
        };
        c.validate_for(request)?;
        Ok(Some(c))
    }
    pub(crate) fn validate_conversion_reference(
        &self,
        line: &Evaluation,
        source_revision: u64,
    ) -> Result<()> {
        line.validate_conversion_snapshot()?;
        if let Some(c) = &line.conversion {
            ensure(
                c.captured_revision <= source_revision,
                "Conversion evidence postdates document pricing",
            )?;
            for rate in [&c.fx, &c.uom].into_iter().flatten() {
                let expected = self.conversion_rate_at(
                    &rate.entry.conversion,
                    &line.request.date,
                    c.captured_revision,
                )?;
                ensure(
                    *rate == expected,
                    "Conversion rate differs from immutable approved history",
                )?;
            }
        }
        Ok(())
    }
    pub(crate) fn validate_conversion_books(&self) -> Result<()> {
        ensure(
            self.conversion_books.len() <= 100
                && self
                    .conversion_books
                    .values()
                    .map(|b| b.terms.entries.len())
                    .sum::<usize>()
                    <= 5000,
            "Conversion book/row limit reached",
        )?;
        for (key, b) in &self.conversion_books {
            self.validate_conversion_terms(&b.terms)?;
            text(&b.owner)?;
            ensure(
                key == &b.terms.id && b.created_revision > 0 && b.created_revision <= self.revision,
                "Conversion identity/creation revision invalid",
            )?;
            let target = conversion_target(key);
            let event = |revision: u64,
                         command: &str,
                         detail: &str,
                         name: &str,
                         reviewer: bool|
             -> Result<()> {
                let a = self
                    .audit
                    .iter()
                    .find(|a| a.revision == revision)
                    .ok_or_else(|| error("Conversion audit missing"))?;
                ensure(
                    a.target.as_deref() == Some(&target)
                        && a.command == command
                        && a.detail == detail
                        && a.actor.name == name
                        && if reviewer {
                            a.actor.role == Role::Approver
                        } else {
                            matches!(a.actor.role, Role::Analyst | Role::Admin)
                        },
                    "Conversion target/actor/command evidence mismatch",
                )
            };
            let created = self
                .audit
                .iter()
                .find(|a| a.revision == b.created_revision)
                .ok_or_else(|| error("Conversion creation audit missing"))?;
            ensure(
                matches!(
                    created.command.as_str(),
                    "conversion_create" | "conversion_import"
                ),
                "Conversion creation command invalid",
            )?;
            event(
                b.created_revision,
                &created.command,
                &conversion_created_detail(b)?,
                &b.owner,
                false,
            )?;
            let mut expected = vec![(b.created_revision, created.command.as_str())];
            if let Some(r) = b.submitted_revision {
                ensure(
                    r > b.created_revision && r <= self.revision,
                    "Conversion submission chronology invalid",
                )?;
                event(
                    r,
                    "conversion_submit",
                    &format!("Submitted conversion {key}"),
                    &b.owner,
                    false,
                )?;
                expected.push((r, "conversion_submit"));
            }
            if let Some(r) = &b.review {
                text(&r.note)?;
                text(&r.actor.name)?;
                ensure(
                    b.submitted_revision.is_some_and(|s| r.revision > s)
                        && r.revision <= self.revision
                        && r.actor.role == Role::Approver
                        && r.actor.name != b.owner,
                    "Conversion requires independent commercial review",
                )?;
                event(
                    r.revision,
                    "conversion_decide",
                    &conversion_decided_detail(b)?,
                    &r.actor.name,
                    true,
                )?;
                if r.approved {
                    self.ensure_conversion_approval(b, r.revision - 1)?;
                }
                expected.push((r.revision, "conversion_decide"));
            }
            if let Some(r) = b.withdrawn_revision {
                ensure(
                    r > b.submitted_revision.unwrap_or(b.created_revision) && r <= self.revision,
                    "Conversion withdrawal chronology invalid",
                )?;
                event(
                    r,
                    "conversion_withdraw",
                    &format!("Withdrew conversion {key}"),
                    &b.owner,
                    false,
                )?;
                expected.push((r, "conversion_withdraw"));
            }
            let valid = match b.status {
                RecordStatus::Draft => {
                    b.version == 1
                        && b.submitted_revision.is_none()
                        && b.review.is_none()
                        && b.withdrawn_revision.is_none()
                }
                RecordStatus::Pending => {
                    b.version == 2
                        && b.submitted_revision.is_some()
                        && b.review.is_none()
                        && b.withdrawn_revision.is_none()
                }
                RecordStatus::Approved | RecordStatus::Rejected => {
                    b.version == 3
                        && b.submitted_revision.is_some()
                        && b.review
                            .as_ref()
                            .is_some_and(|r| r.approved == (b.status == RecordStatus::Approved))
                        && b.withdrawn_revision.is_none()
                }
                RecordStatus::Withdrawn => {
                    b.version == if b.submitted_revision.is_some() { 3 } else { 2 }
                        && b.review.is_none()
                        && b.withdrawn_revision.is_some()
                }
            };
            ensure(valid, "Conversion lifecycle/version invalid")?;
            expected.sort_by_key(|x| x.0);
            let actual: Vec<_> = self
                .audit
                .iter()
                .filter(|a| a.target.as_deref() == Some(&target))
                .map(|a| (a.revision, a.command.as_str()))
                .collect();
            ensure(
                actual == expected,
                "Conversion events were detached or erased",
            )?;
        }
        for a in self.audit.iter().filter(|a| {
            a.command.starts_with("conversion_")
                || a.target
                    .as_deref()
                    .is_some_and(|t| t.starts_with("conversion:"))
        }) {
            let key = a
                .target
                .as_deref()
                .and_then(|t| t.strip_prefix("conversion:"))
                .ok_or_else(|| error("Conversion audit target missing"))?;
            ensure(
                self.conversion_books.contains_key(key),
                "Conversion record was removed",
            )?;
        }
        Ok(())
    }
}
