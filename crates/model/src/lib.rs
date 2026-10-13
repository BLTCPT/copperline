//! Serializable business model. Dates, currencies and units are explicit.
use copperline_money::{Bps, Money, Quantity};
pub use copperline_money::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
mod approval;
pub use approval::*;
mod contracts;
pub use contracts::*;
mod pricing_source;
pub use pricing_source::*;
mod corrections;
pub use corrections::*;
mod record_evidence;
pub use record_evidence::*;
mod settlements;
pub use settlements::*;
pub fn error(s: impl Into<String>) -> Error {
    Error(s.into())
}
pub fn ensure(condition: bool, message: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(error(message))
    }
}
pub fn text(s: &str) -> Result<()> {
    ensure(
        !s.trim().is_empty() && s.len() <= 160 && !s.chars().any(char::is_control),
        "Text must contain 1–160 printable characters",
    )
}
pub fn id(s: &str) -> Result<()> {
    ensure(
        !s.is_empty()
            && s.len() <= 64
            && s.bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c)),
        "ID must use 1–64 letters, digits, hyphen, underscore or period",
    )
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Date(pub String);
impl Date {
    pub fn parse(s: &str) -> Result<Self> {
        let value = Self(s.into());
        value.validate()?;
        Ok(value)
    }
    pub fn validate(&self) -> Result<()> {
        let parts: Vec<_> = self.0.split('-').collect();
        ensure(
            self.0.len() == 10 && parts.len() == 3,
            "Date must be YYYY-MM-DD",
        )?;
        let parse = |n: usize| -> Result<u32> {
            parts
                .get(n)
                .ok_or_else(|| error("Missing date component"))?
                .parse()
                .map_err(|_| error("Invalid date"))
        };
        let (y, m, d) = (parse(0)?, parse(1)?, parse(2)?);
        let days = match m {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 => {
                if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) {
                    29
                } else {
                    28
                }
            }
            _ => 0,
        };
        ensure(
            (2000..=2200).contains(&y)
                && d > 0
                && d <= days
                && self.0 == format!("{y:04}-{m:02}-{d:02}"),
            "Invalid date; supported years 2000–2200",
        )
    }
}
impl std::fmt::Display for Date {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Currency {
    USD,
    EUR,
    GBP,
}
impl std::fmt::Display for Currency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Analyst,
    Sales,
    Approver,
    Finance,
    Admin,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Actor {
    pub name: String,
    pub role: Role,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Product {
    pub id: String,
    pub name: String,
    pub family: String,
    pub unit: String,
    pub currency: Currency,
    pub list_price: Money,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Customer {
    pub id: String,
    pub name: String,
    pub segment: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cost {
    pub product_id: String,
    pub version: u64,
    pub effective_from: Date,
    pub amount: Money,
    pub currency: Currency,
    pub unit: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
pub enum RuleAction {
    Discount(Bps),
    Markup(Bps),
    Fixed(Money),
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub id: String,
    pub version: u64,
    pub name: String,
    pub priority: i32,
    pub product_id: Option<String>,
    pub customer_id: Option<String>,
    pub segment: Option<String>,
    pub effective_from: Date,
    pub effective_to: Option<Date>,
    pub min_quantity: Quantity,
    pub action: RuleAction,
    pub currency: Currency,
}
impl Rule {
    pub fn specificity(&self) -> u8 {
        u8::from(self.customer_id.is_some()) * 4
            + u8::from(self.product_id.is_some()) * 2
            + u8::from(self.segment.is_some())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub min_margin: Bps,
    pub max_discount: Bps,
    pub hard_min_margin: Bps,
}
impl Default for Policy {
    fn default() -> Self {
        Self {
            min_margin: Bps(2500),
            max_discount: Bps(1500),
            hard_min_margin: Bps(0),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PriceRequest {
    pub product_id: String,
    pub customer_id: String,
    pub date: Date,
    pub quantity: Quantity,
    pub discount: Bps,
    pub rebate: Bps,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<PriceSource>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Step {
    pub label: String,
    pub amount: Money,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evaluation {
    pub request: PriceRequest,
    pub currency: Currency,
    pub unit: String,
    pub product_name: String,
    pub list_unit: Money,
    pub rule_unit: Money,
    pub invoice_unit: Money,
    pub rebate_unit: Money,
    pub pocket_unit: Money,
    pub cost_unit: Money,
    pub invoice_total: Money,
    pub rebate_total: Money,
    pub pocket_total: Money,
    pub cost_total: Money,
    pub margin_total: Money,
    pub margin_bps: Option<i32>,
    pub rule: Option<String>,
    pub cost_version: u64,
    pub steps: Vec<Step>,
    pub warnings: Vec<String>,
    pub hard_block: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceEvidence>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Draft,
    Pending,
    Approved,
    Rejected,
    Published,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decision {
    pub actor: String,
    pub note: String,
    pub revision: u64,
    pub approved: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Quote {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub governance: Option<QuoteGovernance>,
    pub id: String,
    pub title: String,
    pub owner: String,
    pub customer_id: String,
    pub date: Date,
    pub version: u64,
    pub source_revision: u64,
    pub policy: Policy,
    pub status: Status,
    pub lines: Vec<Evaluation>,
    pub decisions: Vec<Decision>,
    pub submitted_by: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PriceList {
    pub policy: Policy,
    pub id: String,
    pub name: String,
    pub owner: String,
    pub date: Date,
    pub customer_id: String,
    pub source_revision: u64,
    pub version: u64,
    pub status: Status,
    pub lines: Vec<Evaluation>,
    pub submitted_by: Option<String>,
    pub decisions: Vec<Decision>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Audit {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    pub revision: u64,
    pub actor: Actor,
    pub command: String,
    pub detail: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Receipt {
    pub request_id: String,
    pub revision: u64,
    pub fingerprint: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub quote_approval_policies: Vec<QuoteApprovalPolicy>,
    pub schema_version: u32,
    pub revision: u64,
    pub products: BTreeMap<String, Product>,
    pub customers: BTreeMap<String, Customer>,
    pub costs: Vec<Cost>,
    pub rules: Vec<Rule>,
    pub quotes: BTreeMap<String, Quote>,
    pub price_lists: BTreeMap<String, PriceList>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub grid: BTreeMap<String, GridItem>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub agreements: BTreeMap<String, Agreement>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub transactions: BTreeMap<String, Transaction>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub transaction_revisions: BTreeMap<String, u64>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub accruals: BTreeMap<String, Accrual>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub corrections: BTreeMap<String, Correction>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub settlements: BTreeMap<String, Settlement>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub price_contracts: BTreeMap<String, PriceContract>,
    pub policy: Policy,
    pub audit: Vec<Audit>,
    pub receipts: Vec<Receipt>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            quote_approval_policies: vec![],
            schema_version: 1,
            revision: 0,
            products: BTreeMap::new(),
            customers: BTreeMap::new(),
            costs: vec![],
            rules: vec![],
            quotes: BTreeMap::new(),
            price_lists: BTreeMap::new(),
            grid: BTreeMap::new(),
            agreements: BTreeMap::new(),
            transactions: BTreeMap::new(),
            transaction_revisions: BTreeMap::new(),
            accruals: BTreeMap::new(),
            corrections: BTreeMap::new(),
            settlements: BTreeMap::new(),
            price_contracts: BTreeMap::new(),
            policy: Policy::default(),
            audit: vec![],
            receipts: vec![],
        }
    }
}
impl Product {
    pub fn validate(&self) -> Result<()> {
        id(&self.id)?;
        text(&self.name)?;
        text(&self.family)?;
        id(&self.unit)?;
        self.list_price.nonnegative()?;
        Ok(())
    }
}
impl Customer {
    pub fn validate(&self) -> Result<()> {
        id(&self.id)?;
        text(&self.name)?;
        id(&self.segment)
    }
}
impl State {
    pub fn validate(&self) -> Result<()> {
        ensure(self.schema_version == 1, "Unsupported store schema")?;
        ensure(
            self.products.len() <= 10_000
                && self.customers.len() <= 10_000
                && self.rules.len() <= 10_000
                && self.costs.len() <= 50_000
                && self.quotes.len() <= 10_000
                && self.price_lists.len() <= 1000
                && self.audit.len() <= 100_000
                && self.receipts.len() <= 100_000,
            "Workspace limit reached",
        )?;
        self.policy.min_margin.validate()?;
        self.policy.max_discount.validate()?;
        self.policy.hard_min_margin.validate()?;
        ensure(
            self.policy.hard_min_margin <= self.policy.min_margin,
            "Hard margin floor cannot exceed approval floor",
        )?;
        for (key, p) in &self.products {
            p.validate()?;
            ensure(key == &p.id, "Product key mismatch")?;
        }
        for (key, c) in &self.customers {
            c.validate()?;
            ensure(key == &c.id, "Customer key mismatch")?;
        }
        let mut seen = std::collections::BTreeSet::new();
        for c in &self.costs {
            let p = self
                .products
                .get(&c.product_id)
                .ok_or_else(|| error("Unknown cost product"))?;
            ensure(
                c.currency == p.currency && c.unit == p.unit,
                "Cost currency/unit mismatch",
            )?;
            ensure(
                c.version > 0 && seen.insert((c.product_id.clone(), c.version)),
                "Duplicate or zero cost version",
            )?;
            c.effective_from.validate()?;
            c.amount.nonnegative()?;
        }
        let mut seen = std::collections::BTreeSet::new();
        for r in &self.rules {
            id(&r.id)?;
            text(&r.name)?;
            ensure(
                r.version > 0 && seen.insert((r.id.clone(), r.version)),
                "Duplicate or zero rule version",
            )?;
            r.effective_from.validate()?;
            if let Some(end) = &r.effective_to {
                end.validate()?;
                ensure(
                    end > &r.effective_from,
                    "Rule end is exclusive and must follow start",
                )?;
            }
            r.min_quantity.validate()?;
            if let Some(p) = &r.product_id {
                let p = self
                    .products
                    .get(p)
                    .ok_or_else(|| error("Unknown rule product"))?;
                ensure(p.currency == r.currency, "Rule currency mismatch")?;
            }
            if let Some(c) = &r.customer_id {
                ensure(self.customers.contains_key(c), "Unknown rule customer")?;
            }
            if let Some(s) = &r.segment {
                id(s)?;
            }
            match r.action {
                RuleAction::Discount(b) | RuleAction::Markup(b) => b.validate()?,
                RuleAction::Fixed(m) => {
                    m.nonnegative()?;
                }
            }
        }
        for q in self.quotes.values() {
            id(&q.id)?;
            text(&q.title)?;
            q.date.validate()?;
            ensure(q.lines.len() <= 200, "Quote line limit")?;
            ensure(
                self.customers.contains_key(&q.customer_id),
                "Unknown quote customer",
            )?;
        }
        self.validate_price_contracts()?;
        self.validate_documents()?;
        self.validate_quote_governance()?;
        self.validate_grid()?;
        self.validate_commercial()?;
        self.validate_settlements()?;
        Ok(())
    }
}

impl Policy {
    pub fn validate(&self) -> Result<()> {
        self.min_margin.validate()?;
        self.max_discount.validate()?;
        self.hard_min_margin.validate()?;
        ensure(
            self.hard_min_margin <= self.min_margin,
            "Hard margin floor cannot exceed approval floor",
        )
    }
}
impl Evaluation {
    pub fn validate_snapshot(&self) -> Result<()> {
        self.validate_pricing_source_snapshot()?;
        id(&self.request.product_id)?;
        id(&self.request.customer_id)?;
        self.request.date.validate()?;
        self.request.quantity.validate()?;
        self.request.discount.validate()?;
        self.request.rebate.validate()?;
        id(&self.unit)?;
        text(&self.product_name)?;
        ensure(self.cost_version > 0, "Snapshot requires cost version")?;
        for m in [
            self.list_unit,
            self.rule_unit,
            self.invoice_unit,
            self.rebate_unit,
            self.pocket_unit,
            self.cost_unit,
        ] {
            ensure(m.0 >= 0, "Snapshot amounts must be nonnegative")?;
        }
        ensure(
            self.invoice_unit
                == self
                    .rule_unit
                    .sub(self.rule_unit.percent(self.request.discount)?)?,
            "Snapshot invoice arithmetic mismatch",
        )?;
        ensure(
            self.rebate_unit == self.invoice_unit.percent(self.request.rebate)?
                && self.pocket_unit == self.invoice_unit.sub(self.rebate_unit)?,
            "Snapshot unit rebate mismatch",
        )?;
        ensure(
            self.invoice_total == self.invoice_unit.extend(self.request.quantity)?
                && self.cost_total == self.cost_unit.extend(self.request.quantity)?,
            "Snapshot extension mismatch",
        )?;
        ensure(
            self.rebate_total == self.invoice_total.percent(self.request.rebate)?
                && self.pocket_total == self.invoice_total.sub(self.rebate_total)?
                && self.margin_total == self.pocket_total.sub(self.cost_total)?,
            "Snapshot waterfall mismatch",
        )?;
        let expected = if self.pocket_total.0 > 0 {
            Some(
                i32::try_from(copperline_money::ratio(
                    self.margin_total.0,
                    10_000,
                    self.pocket_total.0,
                )?)
                .map_err(|_| error("Margin overflow"))?,
            )
        } else {
            None
        };
        ensure(
            self.margin_bps == expected,
            "Snapshot margin ratio mismatch",
        )?;
        ensure(
            self.warnings.len() <= 10 && self.steps.len() <= 20,
            "Snapshot explanations exceed limits",
        )?;
        for w in &self.warnings {
            text(w)?;
        }
        for s in &self.steps {
            text(&s.label)?;
        }
        Ok(())
    }
    pub fn check_policy(&self, policy: &Policy) -> Result<()> {
        let below = i128::from(self.margin_total.0) * 10_000
            < i128::from(self.pocket_total.0) * i128::from(policy.hard_min_margin.0);
        ensure(
            self.hard_block == (self.pocket_total.0 == 0 || below),
            "Snapshot hard guardrail mismatch",
        )
    }
}
#[allow(clippy::too_many_arguments)]
fn document_valid(
    status: Status,
    owner: &str,
    submitter: &Option<String>,
    decisions: &[Decision],
    version: u64,
    source: u64,
    current: u64,
    lines: &[Evaluation],
) -> Result<()> {
    text(owner)?;
    ensure(
        version > 0 && source <= current,
        "Invalid document version or source revision",
    )?;
    ensure(lines.len() <= 10_000, "Document line limit exceeded")?;
    if status != Status::Draft {
        ensure(!lines.is_empty(), "Non-draft document requires lines")?;
        text(
            submitter
                .as_deref()
                .ok_or_else(|| error("Non-draft document requires submitter"))?,
        )?;
        ensure(
            !lines.iter().any(|l| l.hard_block),
            "Submitted document violates hard guardrail",
        )?;
    }
    let mut previous_decision = 0;
    for d in decisions {
        ensure(
            d.revision > previous_decision,
            "Decision revisions must increase",
        )?;
        previous_decision = d.revision;
        text(&d.actor)?;
        text(&d.note)?;
        ensure(
            d.revision > 0 && d.revision <= current,
            "Invalid approval decision",
        )?;
    }
    if matches!(
        status,
        Status::Approved | Status::Published | Status::Rejected
    ) {
        let d = decisions
            .last()
            .ok_or_else(|| error("Decided document requires a decision"))?;
        ensure(
            d.revision > source,
            "Current approval predates pricing data",
        )?;
        ensure(
            d.approved == (status != Status::Rejected)
                && d.actor != owner
                && Some(&d.actor) != submitter.as_ref(),
            "Decision does not match document status or submitter",
        )?;
    }
    let mut currency = None;
    let (mut invoice, mut rebate, mut pocket, mut cost) = (Money(0), Money(0), Money(0), Money(0));
    for l in lines {
        invoice = invoice.add(l.invoice_total)?;
        rebate = rebate.add(l.rebate_total)?;
        pocket = pocket.add(l.pocket_total)?;
        cost = cost.add(l.cost_total)?;
        l.validate_snapshot()?;
        if let Some(c) = currency {
            ensure(c == l.currency, "Mixed currency document")?;
        }
        currency = Some(l.currency);
    }
    let margin = pocket.sub(cost)?;
    ensure(invoice.sub(rebate)? == pocket, "Aggregate pocket mismatch")?;
    if pocket.0 > 0 {
        i32::try_from(copperline_money::ratio(margin.0, 10_000, pocket.0)?)
            .map_err(|_| error("Aggregate margin overflow"))?;
    }
    Ok(())
}
impl State {
    pub fn validate_documents(&self) -> Result<()> {
        for (key, q) in &self.quotes {
            ensure(key == &q.id, "Quote key mismatch")?;
            ensure(
                q.status != Status::Published,
                "Quotes cannot use list published status",
            )?;
            q.policy.validate()?;
            self.validate_decision_audit(&q.decisions, "quote_decide", &quote_target(&q.id))?;
            document_valid(
                q.status,
                &q.owner,
                &q.submitted_by,
                &q.decisions,
                q.version,
                q.source_revision,
                self.revision,
                &q.lines,
            )?;
            for l in &q.lines {
                ensure(
                    l.request.customer_id == q.customer_id
                        && l.request.date == q.date
                        && self.products.contains_key(&l.request.product_id),
                    "Quote line context mismatch",
                )?;
                l.check_policy(&q.policy)?;
                self.validate_snapshot_references(l, q.source_revision)?;
            }
        }
        for (key, l) in &self.price_lists {
            id(&l.id)?;
            text(&l.name)?;
            l.date.validate()?;
            ensure(
                key == &l.id && self.customers.contains_key(&l.customer_id),
                "Price list identity mismatch",
            )?;
            l.policy.validate()?;
            self.validate_decision_audit(&l.decisions, "list_decide", &format!("list:{}", l.id))?;
            document_valid(
                l.status,
                &l.owner,
                &l.submitted_by,
                &l.decisions,
                l.version,
                l.source_revision,
                self.revision,
                &l.lines,
            )?;
            for line in &l.lines {
                line.check_policy(&l.policy)?;
                self.validate_snapshot_references(line, l.source_revision)?;
                ensure(
                    line.request.customer_id == l.customer_id
                        && line.request.date == l.date
                        && self.products.contains_key(&line.request.product_id),
                    "List line context mismatch",
                )?;
            }
        }
        ensure(
            self.audit.len() == self.receipts.len() && self.audit.len() as u64 == self.revision,
            "Audit/receipt revision count mismatch",
        )?;
        let mut ids = std::collections::BTreeSet::new();
        for (index, (a, r)) in self.audit.iter().zip(&self.receipts).enumerate() {
            ensure(
                a.revision == index as u64 + 1 && r.revision == a.revision,
                "Audit revision order mismatch",
            )?;
            text(&a.actor.name)?;
            text(&a.command)?;
            text(&a.detail)?;
            id(&r.request_id)?;
            ensure(
                ids.insert(&r.request_id)
                    && r.fingerprint.len() == 64
                    && r.fingerprint.bytes().all(|c| c.is_ascii_hexdigit()),
                "Invalid receipt",
            )?;
        }
        Ok(())
    }
}

impl State {
    fn validate_snapshot_references(&self, l: &Evaluation, source_revision: u64) -> Result<()> {
        self.validate_pricing_source_reference(l, source_revision)?;
        ensure(
            self.customers.contains_key(&l.request.customer_id),
            "Snapshot customer missing",
        )?;
        let product = self
            .products
            .get(&l.request.product_id)
            .ok_or_else(|| error("Snapshot product missing"))?;
        ensure(
            l.currency == product.currency && l.unit == product.unit,
            "Snapshot currency/unit mismatch",
        )?;
        let cost = self
            .costs
            .iter()
            .find(|c| c.product_id == l.request.product_id && c.version == l.cost_version)
            .ok_or_else(|| error("Snapshot cost version missing"))?;
        ensure(
            cost.amount == l.cost_unit
                && cost.currency == l.currency
                && cost.unit == l.unit
                && cost.effective_from <= l.request.date,
            "Snapshot cost reference mismatch",
        )
    }
}

impl State {
    fn validate_decision_audit(
        &self,
        decisions: &[Decision],
        command: &str,
        target: &str,
    ) -> Result<()> {
        for d in decisions {
            let a = self
                .audit
                .iter()
                .find(|a| a.revision == d.revision)
                .ok_or_else(|| error("Decision audit missing"))?;
            ensure(
                a.actor.name == d.actor
                    && (matches!(a.actor.role, Role::Approver | Role::Admin)
                        || (command == "quote_decide" && a.actor.role == Role::Finance))
                    && a.command == command
                    && a.target.as_deref().is_none_or(|t| t == target),
                "Decision actor does not match approval audit",
            )?;
        }
        Ok(())
    }
}

/// A live grid keeps the candidate separate from the last independently approved price.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovedPrice {
    pub evaluation: Evaluation,
    pub policy: Policy,
    pub source_revision: u64,
    pub decision: Decision,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_version: Option<u64>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GridItem {
    pub id: String,
    pub owner: String,
    pub version: u64,
    pub source_revision: u64,
    pub policy: Policy,
    pub status: Status,
    pub candidate: Evaluation,
    pub submitted_by: Option<String>,
    pub decisions: Vec<Decision>,
    pub last_approved: Option<ApprovedPrice>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub approved_history: BTreeMap<u64, ApprovedPrice>,
}
impl State {
    fn validate_grid(&self) -> Result<()> {
        ensure(self.grid.len() <= 10_000, "Live grid item limit reached")?;
        for (key, item) in &self.grid {
            id(&item.id)?;
            ensure(
                key == &item.id && item.status != Status::Published,
                "Invalid grid item identity/status",
            )?;
            item.policy.validate()?;
            document_valid(
                item.status,
                &item.owner,
                &item.submitted_by,
                &item.decisions,
                item.version,
                item.source_revision,
                self.revision,
                std::slice::from_ref(&item.candidate),
            )?;
            self.validate_snapshot_references(&item.candidate, item.source_revision)?;
            item.candidate.check_policy(&item.policy)?;
            self.validate_decision_audit(
                &item.decisions,
                "grid_decide",
                &format!("grid:{}", item.id),
            )?;
            ensure(
                item.last_approved.as_ref().map(|p| &p.decision)
                    == item.decisions.iter().rev().find(|d| d.approved),
                "Active grid price must preserve the latest approved decision",
            )?;
            if let Some(approved) = &item.last_approved {
                approved.policy.validate()?;
                approved.evaluation.validate_snapshot()?;
                approved.evaluation.check_policy(&approved.policy)?;
                self.validate_snapshot_references(&approved.evaluation, approved.source_revision)?;
                self.validate_decision_audit(
                    std::slice::from_ref(&approved.decision),
                    "grid_decide",
                    &format!("grid:{}", item.id),
                )?;
                ensure(
                    approved.decision.approved
                        && approved.decision.revision > approved.source_revision
                        && approved.source_revision <= self.revision
                        && !approved.evaluation.hard_block,
                    "Invalid approved grid price",
                )?;
                ensure(
                    approved.evaluation.request.product_id == item.candidate.request.product_id
                        && approved.evaluation.request.customer_id
                            == item.candidate.request.customer_id,
                    "Grid item identity cannot change",
                )?;
            }
            if item.status == Status::Approved {
                let approved = item
                    .last_approved
                    .as_ref()
                    .ok_or_else(|| error("Approved grid item has no active price"))?;
                ensure(
                    approved.evaluation == item.candidate
                        && approved.source_revision == item.source_revision
                        && approved.policy == item.policy
                        && item.decisions.last() == Some(&approved.decision),
                    "Active grid price differs from approved candidate",
                )?;
            }
            self.validate_grid_source_history(item)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RebateTier {
    pub threshold: Money,
    pub rate: Bps,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgreementTerms {
    pub id: String,
    pub name: String,
    pub customer_id: String,
    pub currency: Currency,
    pub start: Date,
    pub end: Date,
    pub tiers: Vec<RebateTier>,
}
impl AgreementTerms {
    pub fn validate(&self) -> Result<()> {
        id(&self.id)?;
        text(&self.name)?;
        id(&self.customer_id)?;
        self.start.validate()?;
        self.end.validate()?;
        ensure(
            self.start < self.end,
            "Agreement end is exclusive and must follow start",
        )?;
        ensure(
            !self.tiers.is_empty() && self.tiers.len() <= 20,
            "Agreement requires 1–20 tiers",
        )?;
        let mut previous = None;
        let mut previous_rate = Bps(0);
        for t in &self.tiers {
            t.threshold.nonnegative()?;
            t.rate.validate()?;
            if let Some(p) = previous {
                ensure(t.threshold > p, "Tier thresholds must strictly increase")?;
            } else {
                ensure(t.threshold == Money(0), "First tier must start at zero")?;
            }
            ensure(t.rate >= previous_rate, "Tier rates must not decrease")?;
            previous = Some(t.threshold);
            previous_rate = t.rate;
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Agreement {
    pub terms: AgreementTerms,
    pub owner: String,
    pub version: u64,
    pub source_revision: u64,
    pub status: Status,
    pub submitted_by: Option<String>,
    pub decisions: Vec<Decision>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Transaction {
    pub id: String,
    pub source: String,
    pub product_id: String,
    pub customer_id: String,
    pub date: Date,
    pub quantity: Quantity,
    pub unit: String,
    pub currency: Currency,
    pub invoice_total: Money,
    pub cost_total: Money,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Accrual {
    pub id: String,
    pub agreement_id: String,
    pub agreement_version: u64,
    pub source_revision: u64,
    pub currency: Currency,
    pub transaction_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub correction_ids: Vec<String>,
    pub eligible_invoice: Money,
    pub rate: Bps,
    pub rebate: Money,
}
impl State {
    fn validate_commercial(&self) -> Result<()> {
        ensure(
            self.agreements.len() <= 1000
                && self.transactions.len() <= 50_000
                && self.accruals.len() <= 10_000,
            "Commercial dataset limit reached",
        )?;
        ensure(
            self.transactions.len() == self.transaction_revisions.len(),
            "Transaction source revision map mismatch",
        )?;
        self.validate_corrections()?;
        let mut ingestion_counts: BTreeMap<u64, usize> = BTreeMap::new();
        for revision in self.transaction_revisions.values() {
            *ingestion_counts.entry(*revision).or_default() += 1;
        }
        let mut groups: BTreeMap<(String, String), (Money, Money)> = BTreeMap::new();
        for (key, t) in &self.transactions {
            let source = *self
                .transaction_revisions
                .get(key)
                .ok_or_else(|| error("Transaction source revision missing"))?;
            let a = self
                .audit
                .iter()
                .find(|a| a.revision == source)
                .ok_or_else(|| error("Transaction ingestion audit missing"))?;
            let ordinary_ingestion = matches!(
                a.command.as_str(),
                "transaction_add" | "transactions_import"
            ) && matches!(a.actor.role, Role::Analyst | Role::Admin);
            let corrected_ingestion = a.command == "correction_decide"
                && a.actor.role == Role::Finance
                && a.target
                    .as_deref()
                    .and_then(|target| target.strip_prefix("correction:"))
                    .and_then(|key| self.corrections.get(key))
                    .is_some_and(|c| {
                        c.approved_revision() == Some(source)
                            && c.proposal.replacement.as_ref() == Some(t)
                    });
            ensure(
                ordinary_ingestion || corrected_ingestion,
                "Transaction ingestion audit invalid",
            )?;
            if a.command == "transaction_add" {
                ensure(
                    a.detail == format!("Recorded invoice {}", t.id)
                        && a.target
                            .as_deref()
                            .is_none_or(|target| target == format!("invoice:{}", t.id)),
                    "Invoice ingestion target or detail mismatch",
                )?;
            } else if a.command == "transactions_import" {
                let count = ingestion_counts.get(&source).copied().unwrap_or(0);
                ensure(
                    a.target.is_none()
                        && a.detail == format!("Imported {count} invoices atomically"),
                    "Invoice import count or audit mismatch",
                )?;
            }
            let g = groups
                .entry((t.customer_id.clone(), t.currency.to_string()))
                .or_insert((Money(0), Money(0)));
            g.0 = g.0.add(t.invoice_total)?;
            g.1 = g.1.add(t.cost_total)?;
            ensure(key == &t.id, "Transaction key mismatch")?;
            self.validate_transaction(t)?;
        }
        for c in self
            .corrections
            .values()
            .filter(|c| c.status == RecordStatus::Approved)
        {
            let g = groups
                .get_mut(&(
                    c.original.customer_id.clone(),
                    c.original.currency.to_string(),
                ))
                .ok_or_else(|| error("Reversal analytics group missing"))?;
            g.0 = g.0.sub(c.original.invoice_total)?;
            g.1 = g.1.sub(c.original.cost_total)?;
            ensure(
                g.0.0 >= 0 && g.1.0 >= 0,
                "Reversal exceeds recorded invoice totals",
            )?;
        }
        for (invoice, cost) in groups.values() {
            let margin = invoice.sub(*cost)?;
            if invoice.0 > 0 {
                i32::try_from(copperline_money::ratio(margin.0, 10_000, invoice.0)?)
                    .map_err(|_| error("Invoice analytics margin overflow"))?;
            }
        }
        for (key, a) in &self.agreements {
            a.terms.validate()?;
            ensure(
                key == &a.terms.id
                    && self.customers.contains_key(&a.terms.customer_id)
                    && a.version > 0
                    && a.source_revision <= self.revision,
                "Agreement identity/version invalid",
            )?;
            text(&a.owner)?;
            ensure(a.status != Status::Published, "Agreement status invalid")?;
            if a.status != Status::Draft {
                text(
                    a.submitted_by
                        .as_deref()
                        .ok_or_else(|| error("Agreement submitter missing"))?,
                )?;
            }
            self.validate_decision_audit(
                &a.decisions,
                "agreement_decide",
                &format!("agreement:{}", a.terms.id),
            )?;
            let mut previous = 0;
            for d in &a.decisions {
                ensure(
                    d.revision > previous
                        && d.revision <= self.revision
                        && d.actor != a.owner
                        && Some(&d.actor) != a.submitted_by.as_ref(),
                    "Agreement decision invalid",
                )?;
                previous = d.revision;
            }
            if matches!(a.status, Status::Approved | Status::Rejected) {
                let d = a
                    .decisions
                    .last()
                    .ok_or_else(|| error("Agreement decision missing"))?;
                ensure(
                    d.approved == (a.status == Status::Approved) && d.revision > a.source_revision,
                    "Agreement decision mismatches status",
                )?;
            }
        }
        let approved: Vec<_> = self
            .agreements
            .values()
            .filter(|a| a.status == Status::Approved)
            .collect();
        for (i, a) in approved.iter().enumerate() {
            for b in approved.iter().skip(i + 1) {
                ensure(
                    !(a.terms.customer_id == b.terms.customer_id
                        && a.terms.currency == b.terms.currency
                        && a.terms.start < b.terms.end
                        && b.terms.start < a.terms.end),
                    "Approved rebate agreements cannot overlap for the same customer/currency",
                )?;
            }
        }
        for (key, a) in &self.accruals {
            id(&a.id)?;
            let revision = a
                .source_revision
                .checked_add(1)
                .ok_or_else(|| error("Accrual creation revision overflow"))?;
            let event = self
                .audit
                .iter()
                .find(|event| event.revision == revision)
                .ok_or_else(|| error("Accrual creation audit missing"))?;
            ensure(
                event.command == "rebate_accrue"
                    && matches!(event.actor.role, Role::Analyst | Role::Admin)
                    && event.detail
                        == format!(
                            "Calculated accrual {}: {}; no payment or posting",
                            a.id, a.rebate
                        )
                    && event
                        .target
                        .as_deref()
                        .is_none_or(|target| target == format!("accrual:{}", a.id)),
                "Accrual author, target or arithmetic audit mismatch",
            )?;
            ensure(
                key == &a.id && a.source_revision < self.revision,
                "Accrual identity/source invalid",
            )?;
            let agreement = self
                .agreements
                .get(&a.agreement_id)
                .ok_or_else(|| error("Accrual agreement missing"))?;
            ensure(
                agreement
                    .decisions
                    .last()
                    .is_some_and(|d| d.approved && d.revision <= a.source_revision),
                "Accrual predates agreement approval",
            )?;
            ensure(
                agreement.status == Status::Approved
                    && agreement.version == a.agreement_version
                    && a.currency == agreement.terms.currency,
                "Accrual agreement mismatch",
            )?;
            ensure(
                !a.transaction_ids.is_empty() && a.transaction_ids.len() <= 50_000,
                "Accrual must contain transactions",
            )?;
            let basis = self.accrual_basis(&a.agreement_id, a.source_revision)?;
            ensure(
                a.transaction_ids == basis.transaction_ids
                    && a.correction_ids == basis.correction_ids,
                "Accrual must include every eligible invoice and approved original reversal known at its source revision",
            )?;
            let invoice = basis.eligible_invoice;
            let rate = agreement
                .terms
                .tiers
                .iter()
                .filter(|t| t.threshold <= invoice)
                .map(|t| t.rate)
                .next_back()
                .ok_or_else(|| error("No rebate tier"))?;
            ensure(
                invoice == a.eligible_invoice
                    && a.rate == rate
                    && a.rebate == invoice.percent(rate)?,
                "Accrual arithmetic mismatch",
            )?;
        }
        for event in &self.audit {
            if let Some(key) = event
                .target
                .as_deref()
                .and_then(|target| target.strip_prefix("invoice:"))
            {
                ensure(
                    event.command == "transaction_add"
                        && self.transactions.contains_key(key)
                        && self.transaction_revisions.get(key) == Some(&event.revision),
                    "Invoice audit record was erased or detached",
                )?;
            }
            if let Some(key) = event
                .target
                .as_deref()
                .and_then(|target| target.strip_prefix("accrual:"))
            {
                ensure(
                    event.command == "rebate_accrue"
                        && self.accruals.get(key).is_some_and(|a| {
                            a.source_revision.checked_add(1) == Some(event.revision)
                        }),
                    "Accrual audit record was erased or detached",
                )?;
            }
        }
        Ok(())
    }
}
