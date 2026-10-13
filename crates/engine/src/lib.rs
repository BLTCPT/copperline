//! Shared transactional command boundary for desktop, CLI and local JSON API.
use copperline_model::*;
use copperline_money::{Bps, Money, Quantity};
pub use copperline_pricing::{Comparison, Totals, compare, evaluate, totals};
use copperline_storage::Store;
use serde::{Deserialize, Serialize};
use std::path::Path;
mod backup;
mod bookings;
mod commercial;
mod contracts;
mod conversions;
mod corrections;
mod csv;
mod governance;
mod grid;
mod settlements;
pub use commercial::analytics;
pub use copperline_storage::backup::{BackupSummary, RestoreSummary};
pub use csv::{
    export_booking, export_contract, export_conversion, export_quote, export_report,
    export_settlement, import_conversions, import_products,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "command",
    content = "params",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Command {
    QuoteBook {
        request: BookingRequest,
    },
    ConversionCreate {
        terms: ConversionTerms,
    },
    ConversionImport {
        id: String,
        name: String,
        csv: String,
    },
    ConversionSubmit {
        id: String,
        version: u64,
    },
    ConversionDecide {
        id: String,
        version: u64,
        approve: bool,
        note: String,
    },
    ConversionWithdraw {
        id: String,
        version: u64,
    },
    ReportConvert {
        id: String,
        request: ReportingRequest,
    },
    QuoteReplaceLines {
        id: String,
        version: u64,
        replacements: Vec<QuoteLineReplacement>,
    },
    ContractCreate {
        terms: ContractTerms,
    },
    ContractSubmit {
        id: String,
        edition: u64,
        version: u64,
    },
    ContractDecide {
        id: String,
        edition: u64,
        version: u64,
        approve: bool,
        note: String,
    },
    ContractWithdraw {
        id: String,
        edition: u64,
        version: u64,
    },
    SeedDemo,
    CorrectionCreate {
        proposal: CorrectionProposal,
    },
    CorrectionSubmit {
        id: String,
        version: u64,
    },
    CorrectionDecide {
        id: String,
        version: u64,
        approve: bool,
        note: String,
    },
    CorrectionWithdraw {
        id: String,
        version: u64,
    },
    SettlementCreate {
        id: String,
        accrual_id: String,
        reason: String,
    },
    SettlementSubmit {
        id: String,
        version: u64,
    },
    SettlementDecide {
        id: String,
        version: u64,
        approve: bool,
        note: String,
    },
    SettlementWithdraw {
        id: String,
        version: u64,
    },
    QuoteApprovalPolicySet {
        expected_version: u64,
        thresholds: ApprovalThresholds,
    },
    AgreementCreate {
        terms: AgreementTerms,
    },
    AgreementSubmit {
        id: String,
        version: u64,
    },
    AgreementDecide {
        id: String,
        version: u64,
        approve: bool,
        note: String,
    },
    TransactionAdd(Transaction),
    TransactionsImport {
        csv: String,
    },
    RebateAccrue {
        id: String,
        agreement_id: String,
        version: u64,
    },
    GridCalculate {
        id: String,
        version: u64,
        request: PriceRequest,
    },
    GridSubmit {
        id: String,
        version: u64,
    },
    GridDecide {
        id: String,
        version: u64,
        approve: bool,
        note: String,
    },
    ProductPut(Product),
    CustomerPut(Customer),
    CostAdd(Cost),
    RuleAdd(Rule),
    PolicySet(Policy),
    ProductsImport {
        csv: String,
    },
    QuoteCreate {
        id: String,
        title: String,
        customer_id: String,
        date: Date,
    },
    QuoteAddLine {
        id: String,
        version: u64,
        request: PriceRequest,
    },
    QuoteReplaceLine {
        id: String,
        version: u64,
        index: usize,
        request: PriceRequest,
    },
    QuoteRemoveLine {
        id: String,
        version: u64,
        index: usize,
    },
    QuoteReprice {
        id: String,
        version: u64,
    },
    QuoteSubmit {
        id: String,
        version: u64,
    },
    QuoteDecide {
        id: String,
        version: u64,
        approve: bool,
        note: String,
    },
    QuoteRevise {
        id: String,
        version: u64,
    },
    ListCalculate {
        id: String,
        name: String,
        customer_id: String,
        date: Date,
        quantity: Quantity,
        currency: Currency,
    },
    ListSubmit {
        id: String,
        version: u64,
    },
    ListDecide {
        id: String,
        version: u64,
        approve: bool,
        note: String,
    },
    ListPublish {
        id: String,
        version: u64,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuoteLineReplacement {
    pub index: usize,
    pub request: PriceRequest,
}
impl Command {
    pub fn name(&self) -> &'static str {
        match self {
            Self::QuoteBook { .. } => "quote_book",
            Self::ConversionCreate { .. } => "conversion_create",
            Self::ConversionImport { .. } => "conversion_import",
            Self::ConversionSubmit { .. } => "conversion_submit",
            Self::ConversionDecide { .. } => "conversion_decide",
            Self::ConversionWithdraw { .. } => "conversion_withdraw",
            Self::ReportConvert { .. } => "report_convert",
            Self::QuoteReplaceLines { .. } => "quote_replace_lines",
            Self::ContractCreate { .. } => "contract_create",
            Self::ContractSubmit { .. } => "contract_submit",
            Self::ContractDecide { .. } => "contract_decide",
            Self::ContractWithdraw { .. } => "contract_withdraw",
            Self::SeedDemo => "seed_demo",
            Self::CorrectionCreate { .. } => "correction_create",
            Self::CorrectionSubmit { .. } => "correction_submit",
            Self::CorrectionDecide { .. } => "correction_decide",
            Self::CorrectionWithdraw { .. } => "correction_withdraw",
            Self::SettlementCreate { .. } => "settlement_create",
            Self::SettlementSubmit { .. } => "settlement_submit",
            Self::SettlementDecide { .. } => "settlement_decide",
            Self::SettlementWithdraw { .. } => "settlement_withdraw",
            Self::QuoteApprovalPolicySet { .. } => "quote_approval_policy_set",
            Self::AgreementCreate { .. } => "agreement_create",
            Self::AgreementSubmit { .. } => "agreement_submit",
            Self::AgreementDecide { .. } => "agreement_decide",
            Self::TransactionAdd(_) => "transaction_add",
            Self::TransactionsImport { .. } => "transactions_import",
            Self::RebateAccrue { .. } => "rebate_accrue",
            Self::GridCalculate { .. } => "grid_calculate",
            Self::GridSubmit { .. } => "grid_submit",
            Self::GridDecide { .. } => "grid_decide",
            Self::ProductPut(_) => "product_put",
            Self::CustomerPut(_) => "customer_put",
            Self::CostAdd(_) => "cost_add",
            Self::RuleAdd(_) => "rule_add",
            Self::PolicySet(_) => "policy_set",
            Self::ProductsImport { .. } => "products_import",
            Self::QuoteCreate { .. } => "quote_create",
            Self::QuoteAddLine { .. } => "quote_add_line",
            Self::QuoteReplaceLine { .. } => "quote_replace_line",
            Self::QuoteRemoveLine { .. } => "quote_remove_line",
            Self::QuoteReprice { .. } => "quote_reprice",
            Self::QuoteSubmit { .. } => "quote_submit",
            Self::QuoteDecide { .. } => "quote_decide",
            Self::QuoteRevise { .. } => "quote_revise",
            Self::ListCalculate { .. } => "list_calculate",
            Self::ListSubmit { .. } => "list_submit",
            Self::ListDecide { .. } => "list_decide",
            Self::ListPublish { .. } => "list_publish",
        }
    }
    fn allowed(&self, role: Role) -> bool {
        match self {
            Self::SeedDemo | Self::PolicySet(_) | Self::QuoteApprovalPolicySet { .. } => {
                role == Role::Admin
            }
            Self::QuoteBook { .. }
            | Self::ConversionCreate { .. }
            | Self::ConversionImport { .. }
            | Self::ConversionSubmit { .. }
            | Self::ConversionWithdraw { .. }
            | Self::ReportConvert { .. }
            | Self::ContractCreate { .. }
            | Self::ContractSubmit { .. }
            | Self::ContractWithdraw { .. }
            | Self::CorrectionCreate { .. }
            | Self::CorrectionSubmit { .. }
            | Self::CorrectionWithdraw { .. }
            | Self::SettlementCreate { .. }
            | Self::SettlementSubmit { .. }
            | Self::SettlementWithdraw { .. }
            | Self::AgreementCreate { .. }
            | Self::AgreementSubmit { .. }
            | Self::TransactionAdd(_)
            | Self::TransactionsImport { .. }
            | Self::RebateAccrue { .. }
            | Self::GridCalculate { .. }
            | Self::GridSubmit { .. }
            | Self::ProductPut(_)
            | Self::CustomerPut(_)
            | Self::CostAdd(_)
            | Self::RuleAdd(_)
            | Self::ProductsImport { .. }
            | Self::ListCalculate { .. }
            | Self::ListSubmit { .. }
            | Self::ListPublish { .. } => matches!(role, Role::Analyst | Role::Admin),
            Self::ConversionDecide { .. } | Self::ContractDecide { .. } => role == Role::Approver,
            Self::CorrectionDecide { .. } | Self::SettlementDecide { .. } => role == Role::Finance,
            Self::AgreementDecide { .. } | Self::GridDecide { .. } | Self::ListDecide { .. } => {
                matches!(role, Role::Approver | Role::Admin)
            }
            Self::QuoteDecide { .. } => {
                matches!(role, Role::Approver | Role::Finance | Role::Admin)
            }
            _ => matches!(role, Role::Sales | Role::Admin),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub request_id: String,
    pub expected_revision: u64,
    pub actor: Actor,
    #[serde(flatten)]
    pub action: Command,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Outcome {
    pub revision: u64,
    pub replayed: bool,
    pub detail: String,
}
pub struct Engine {
    state: State,
    store: Option<Store>,
}
impl Engine {
    pub fn memory() -> Self {
        Self {
            state: State::default(),
            store: None,
        }
    }
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let (mut store, state) = Store::open(path)?;
        if state.revision == 0 && state.audit.is_empty() {
            // Bootstrap only when no committed state exists.
            if !store.path().join("00000000000000000000.json").exists() {
                store.commit(&state)?;
            }
        }
        Ok(Self {
            state,
            store: Some(store),
        })
    }
    pub fn state(&self) -> &State {
        &self.state
    }
    pub fn path(&self) -> Option<&Path> {
        self.store.as_ref().map(Store::path)
    }
    pub fn execute(&mut self, request: Request) -> Result<Outcome> {
        id(&request.request_id)?;
        text(&request.actor.name)?;
        let encoded = serde_json::to_vec(&request).map_err(|e| error(e.to_string()))?;
        ensure(encoded.len() <= 2 * 1024 * 1024, "Command exceeds 2 MiB")?;
        let fingerprint = copperline_storage::fingerprint(&encoded);
        if let Some(receipt) = self
            .state
            .receipts
            .iter()
            .find(|r| r.request_id == request.request_id)
        {
            ensure(
                receipt.fingerprint == fingerprint,
                "Request ID was already used for a different command",
            )?;
            return Ok(Outcome {
                revision: receipt.revision,
                replayed: true,
                detail: "Previously committed command".into(),
            });
        }
        ensure(
            request.expected_revision == self.state.revision,
            "Workspace changed; refresh and retry with the current revision",
        )?;
        ensure(
            request.action.allowed(request.actor.role),
            "Role is not allowed to perform this command",
        )?;
        let mut next = self.state.clone();
        // Escaped dependency panics cannot partially change the current state or durable revision.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            governance::prepare(&mut next, &request.actor, &request.action)?;
            apply(&mut next, &request.actor, &request.action)
        }));
        let detail = match result {
            Ok(r) => r?,
            Err(_) => {
                return Err(error(
                    "Command failed unexpectedly; workspace was not changed",
                ));
            }
        };
        next.revision = self
            .state
            .revision
            .checked_add(1)
            .ok_or_else(|| error("Revision overflow"))?;
        next.audit.push(Audit {
            target: governance::target(&request.action),
            revision: next.revision,
            actor: request.actor.clone(),
            command: request.action.name().into(),
            detail: detail.clone(),
        });
        next.receipts.push(Receipt {
            request_id: request.request_id,
            revision: next.revision,
            fingerprint,
        });
        let validated = next.validated()?;
        if let Some(store) = &mut self.store {
            store.commit_validated(validated)?;
        }
        self.state = next;
        Ok(Outcome {
            revision: self.state.revision,
            replayed: false,
            detail,
        })
    }
    pub fn run(&mut self, actor: Actor, action: Command) -> Result<Outcome> {
        let revision = self.state.revision;
        let next = revision
            .checked_add(1)
            .ok_or_else(|| error("Revision overflow"))?;
        let base = format!("local-{next}-{}", action.name());
        let used: std::collections::BTreeSet<_> = self
            .state
            .receipts
            .iter()
            .map(|receipt| receipt.request_id.as_str())
            .collect();
        // External clients share the idempotency namespace. At most `used.len()`
        // candidates can be occupied, so this bounded search always has a free ID.
        let request_id = (0..=used.len())
            .map(|suffix| {
                if suffix == 0 {
                    base.clone()
                } else {
                    format!("{base}-{suffix}")
                }
            })
            .find(|candidate| !used.contains(candidate.as_str()))
            .ok_or_else(|| error("No unused local request ID"))?;
        self.execute(Request {
            request_id,
            expected_revision: revision,
            actor,
            action,
        })
    }
}
fn next_version(v: u64) -> Result<u64> {
    v.checked_add(1).ok_or_else(|| error("Version overflow"))
}
fn draft(q: &Quote, version: u64) -> Result<()> {
    ensure(q.version == version, "Quote version changed")?;
    ensure(
        q.status == Status::Draft,
        "Quote is locked; revise it before editing",
    )
}
fn quote_mut<'a>(s: &'a mut State, key: &str) -> Result<&'a mut Quote> {
    s.quotes
        .get_mut(key)
        .ok_or_else(|| error("Quote not found"))
}
fn apply(s: &mut State, actor: &Actor, c: &Command) -> Result<String> {
    match c {
        Command::QuoteBook { request } => bookings::apply(s, actor, request),
        Command::ConversionCreate { .. }
        | Command::ConversionImport { .. }
        | Command::ConversionSubmit { .. }
        | Command::ConversionDecide { .. }
        | Command::ConversionWithdraw { .. }
        | Command::ReportConvert { .. } => conversions::apply(s, actor, c),
        Command::QuoteReplaceLines {
            id,
            version,
            replacements,
        } => {
            let q = s.quotes.get(id).ok_or_else(|| error("Quote not found"))?;
            draft(q, *version)?;
            ensure(
                !replacements.is_empty() && replacements.len() <= q.lines.len(),
                "Replace 1 or more existing lines in one batch",
            )?;
            let mut requests: Vec<_> = q.lines.iter().map(|l| l.request.clone()).collect();
            let mut seen = std::collections::BTreeSet::new();
            for replacement in replacements {
                ensure(
                    seen.insert(replacement.index),
                    "Duplicate replacement index",
                )?;
                ensure(
                    replacement.request.customer_id == q.customer_id
                        && replacement.request.date == q.date,
                    "Line customer/date must match quote",
                )?;
                let slot = requests
                    .get_mut(replacement.index)
                    .ok_or_else(|| error("Line index not found"))?;
                *slot = replacement.request.clone();
            }
            let lines = requests
                .iter()
                .map(|r| evaluate(s, r))
                .collect::<Result<Vec<_>>>()?;
            totals(&lines)?;
            let revision = s.revision;
            let policy = s.policy.clone();
            let q = quote_mut(s, id)?;
            q.lines = lines;
            q.version = next_version(q.version)?;
            q.source_revision = revision;
            q.policy = policy;
            Ok(format!(
                "Replaced {} lines and recalculated {id}",
                replacements.len()
            ))
        }
        Command::CorrectionCreate { .. }
        | Command::CorrectionSubmit { .. }
        | Command::CorrectionDecide { .. }
        | Command::CorrectionWithdraw { .. } => corrections::apply(s, actor, c),
        Command::SettlementCreate { .. }
        | Command::SettlementSubmit { .. }
        | Command::SettlementDecide { .. }
        | Command::SettlementWithdraw { .. } => settlements::apply(s, actor, c),
        Command::ContractCreate { .. }
        | Command::ContractSubmit { .. }
        | Command::ContractDecide { .. }
        | Command::ContractWithdraw { .. } => contracts::apply(s, actor, c),
        Command::QuoteApprovalPolicySet { .. } => governance::apply(s, actor, c),
        Command::AgreementCreate { .. }
        | Command::AgreementSubmit { .. }
        | Command::AgreementDecide { .. }
        | Command::TransactionAdd(_)
        | Command::TransactionsImport { .. }
        | Command::RebateAccrue { .. } => commercial::apply(s, actor, c),
        Command::GridCalculate { .. } | Command::GridSubmit { .. } | Command::GridDecide { .. } => {
            grid::apply(s, actor, c)
        }
        Command::SeedDemo => {
            ensure(
                s.products.is_empty() && s.customers.is_empty(),
                "Demo can only seed an empty workspace",
            )?;
            seed(s)?;
            Ok("Loaded synthetic industrial catalog, costs, customers and rules".into())
        }
        Command::ProductPut(p) => {
            p.validate()?;
            if let Some(old) = s.products.get(&p.id) {
                ensure(
                    old.currency == p.currency && old.unit == p.unit,
                    "Existing product currency and unit are immutable",
                )?;
            }
            s.products.insert(p.id.clone(), p.clone());
            Ok(format!("Saved product {}", p.id))
        }
        Command::CustomerPut(customer) => {
            customer.validate()?;
            s.customers.insert(customer.id.clone(), customer.clone());
            Ok(format!("Saved customer {}", customer.id))
        }
        Command::CostAdd(cost) => {
            let latest = s
                .costs
                .iter()
                .filter(|v| v.product_id == cost.product_id)
                .map(|v| v.version)
                .max()
                .unwrap_or(0);
            ensure(
                cost.version == next_version(latest)?,
                "Cost version must be the next sequential version",
            )?;
            s.costs.push(cost.clone());
            Ok(format!("Added cost {} v{}", cost.product_id, cost.version))
        }
        Command::RuleAdd(rule) => {
            let latest = s
                .rules
                .iter()
                .filter(|v| v.id == rule.id)
                .map(|v| v.version)
                .max()
                .unwrap_or(0);
            ensure(
                rule.version == next_version(latest)?,
                "Rule version must be the next sequential version",
            )?;
            s.rules.push(rule.clone());
            Ok(format!("Added rule {} v{}", rule.id, rule.version))
        }
        Command::PolicySet(policy) => {
            s.policy = policy.clone();
            Ok(
                "Changed guardrails; existing documents retain their frozen policy until repriced"
                    .into(),
            )
        }
        Command::ProductsImport { csv } => {
            let products = import_products(csv)?;
            let n = products.len();
            for p in products {
                ensure(
                    !s.products.contains_key(&p.id),
                    "Import contains an existing product ID; use product_put for deliberate updates",
                )?;
                s.products.insert(p.id.clone(), p);
            }
            Ok(format!("Imported {n} products atomically"))
        }
        Command::QuoteCreate {
            id: key,
            title,
            customer_id,
            date,
        } => {
            id(key)?;
            text(title)?;
            date.validate()?;
            ensure(!s.quotes.contains_key(key), "Quote ID already exists")?;
            ensure(s.customers.contains_key(customer_id), "Customer not found")?;
            s.quotes.insert(
                key.clone(),
                Quote {
                    governance: Some(QuoteGovernance {
                        contributors: std::collections::BTreeSet::from([actor.name.clone()]),
                        cycles: vec![],
                    }),
                    id: key.clone(),
                    title: title.clone(),
                    owner: actor.name.clone(),
                    customer_id: customer_id.clone(),
                    date: date.clone(),
                    version: 1,
                    source_revision: s.revision,
                    policy: s.policy.clone(),
                    status: Status::Draft,
                    lines: vec![],
                    decisions: vec![],
                    submitted_by: None,
                },
            );
            Ok(format!("Created quote {key}"))
        }
        Command::QuoteAddLine {
            id,
            version,
            request,
        } => {
            let q = s.quotes.get(id).ok_or_else(|| error("Quote not found"))?;
            draft(q, *version)?;
            ensure(
                request.customer_id == q.customer_id && request.date == q.date,
                "Line customer/date must match quote",
            )?;
            ensure(q.lines.len() < 200, "Quote is limited to 200 lines")?;
            // Refresh existing lines on every draft edit so a document never mixes data revisions.
            let mut requests = q
                .lines
                .iter()
                .map(|l| l.request.clone())
                .collect::<Vec<_>>();
            requests.push(request.clone());
            let lines = requests
                .iter()
                .map(|r| evaluate(s, r))
                .collect::<Result<Vec<_>>>()?;
            totals(&lines)?;
            let revision = s.revision;
            let policy = s.policy.clone();
            let q = quote_mut(s, id)?;
            q.lines = lines;
            q.version = next_version(q.version)?;
            q.source_revision = revision;
            q.policy = policy;
            Ok(format!("Added line and recalculated {id}"))
        }
        Command::QuoteReplaceLine {
            id,
            version,
            index,
            request,
        } => {
            let q = s.quotes.get(id).ok_or_else(|| error("Quote not found"))?;
            draft(q, *version)?;
            ensure(*index < q.lines.len(), "Line index not found")?;
            ensure(
                request.customer_id == q.customer_id && request.date == q.date,
                "Line customer/date must match quote",
            )?;
            let mut requests: Vec<_> = q.lines.iter().map(|l| l.request.clone()).collect();
            let slot = requests
                .get_mut(*index)
                .ok_or_else(|| error("Line index not found"))?;
            *slot = request.clone();
            let lines = requests
                .iter()
                .map(|r| evaluate(s, r))
                .collect::<Result<Vec<_>>>()?;
            totals(&lines)?;
            let revision = s.revision;
            let policy = s.policy.clone();
            let q = quote_mut(s, id)?;
            q.lines = lines;
            q.version = next_version(q.version)?;
            q.source_revision = revision;
            q.policy = policy;
            Ok(format!("Replaced line and recalculated {id}"))
        }
        Command::QuoteRemoveLine { id, version, index } => {
            let q = quote_mut(s, id)?;
            draft(q, *version)?;
            ensure(*index < q.lines.len(), "Line index not found")?;
            q.lines.remove(*index);
            q.version = next_version(q.version)?;
            Ok(format!("Removed line from {id}"))
        }
        Command::QuoteReprice { id, version } => {
            let q = s.quotes.get(id).ok_or_else(|| error("Quote not found"))?;
            draft(q, *version)?;
            let lines = q
                .lines
                .iter()
                .map(|l| evaluate(s, &l.request))
                .collect::<Result<Vec<_>>>()?;
            let rev = s.revision;
            let policy = s.policy.clone();
            let q = quote_mut(s, id)?;
            q.lines = lines;
            q.source_revision = rev;
            q.policy = policy;
            q.version = next_version(q.version)?;
            Ok(format!("Repriced {id} against revision {rev}"))
        }
        Command::QuoteSubmit { .. } | Command::QuoteDecide { .. } => governance::apply(s, actor, c),
        Command::QuoteRevise { id, version } => {
            let q = quote_mut(s, id)?;
            ensure(
                q.version == *version && matches!(q.status, Status::Approved | Status::Rejected),
                "Only an approved or rejected quote can be revised",
            )?;
            q.status = Status::Draft;
            q.submitted_by = None;
            q.version = next_version(q.version)?;
            Ok(format!(
                "Reopened {id}; previous approval no longer applies"
            ))
        }
        Command::ListCalculate {
            id: key,
            name,
            customer_id,
            date,
            quantity,
            currency,
        } => {
            id(key)?;
            text(name)?;
            ensure(
                !s.price_lists.contains_key(key),
                "List ID exists; calculate a new snapshot ID",
            )?;
            ensure(!s.products.is_empty(), "Catalog is empty")?;
            let lines = s
                .products
                .values()
                .filter(|p| p.currency == *currency)
                .map(|p| {
                    evaluate(
                        s,
                        &PriceRequest {
                            source: None,
                            conversion: None,
                            product_id: p.id.clone(),
                            customer_id: customer_id.clone(),
                            date: date.clone(),
                            quantity: *quantity,
                            discount: Bps(0),
                            rebate: Bps(0),
                        },
                    )
                })
                .collect::<Result<Vec<_>>>()?;
            totals(&lines)?;
            s.price_lists.insert(
                key.clone(),
                PriceList {
                    policy: s.policy.clone(),
                    id: key.clone(),
                    name: name.clone(),
                    owner: actor.name.clone(),
                    date: date.clone(),
                    customer_id: customer_id.clone(),
                    source_revision: s.revision,
                    version: 1,
                    status: Status::Draft,
                    lines,
                    submitted_by: None,
                    decisions: vec![],
                },
            );
            Ok(format!("Calculated frozen price list {key}"))
        }
        Command::ListSubmit { id, version } => {
            let list = s
                .price_lists
                .get_mut(id)
                .ok_or_else(|| error("Price list not found"))?;
            ensure(
                list.version == *version && list.status == Status::Draft,
                "List must be the current draft version",
            )?;
            ensure(
                !list.lines.iter().any(|l| l.hard_block),
                "List contains hard guardrail violations",
            )?;
            list.status = Status::Pending;
            list.submitted_by = Some(actor.name.clone());
            list.version = next_version(list.version)?;
            Ok(format!("Submitted list {id}"))
        }
        Command::ListDecide {
            id,
            version,
            approve,
            note,
        } => {
            text(note)?;
            let list = s
                .price_lists
                .get_mut(id)
                .ok_or_else(|| error("Price list not found"))?;
            ensure(
                list.version == *version && list.status == Status::Pending,
                "List must be the current pending version",
            )?;
            ensure(
                list.owner != actor.name && list.submitted_by.as_ref() != Some(&actor.name),
                "A different person must approve this list",
            )?;
            list.status = if *approve {
                Status::Approved
            } else {
                Status::Rejected
            };
            list.decisions.push(Decision {
                actor: actor.name.clone(),
                note: note.clone(),
                revision: s.revision + 1,
                approved: *approve,
            });
            list.version = next_version(list.version)?;
            Ok(format!("Decided list {id}"))
        }
        Command::ListPublish { id, version } => {
            let list = s
                .price_lists
                .get_mut(id)
                .ok_or_else(|| error("Price list not found"))?;
            ensure(
                list.version == *version && list.status == Status::Approved,
                "Only the current approved list can be published",
            )?;
            list.status = Status::Published;
            list.version = next_version(list.version)?;
            Ok(format!("Published list {id} locally"))
        }
    }
}
fn seed(s: &mut State) -> Result<()> {
    let products = [
        (
            "VLV-100",
            "Precision flow valve",
            "Flow control",
            10000,
            6000,
        ),
        (
            "MFC-240",
            "Mass flow controller",
            "Flow control",
            148000,
            89000,
        ),
        ("PMP-080", "Dry vacuum pump", "Vacuum", 465000, 294000),
        (
            "FLT-020",
            "Inline process filter",
            "Consumables",
            8500,
            3900,
        ),
        ("KIT-450", "Chamber service kit", "Service", 128000, 72500),
        (
            "SNS-120",
            "Pressure transducer",
            "Instrumentation",
            64000,
            36500,
        ),
    ];
    for (id, name, family, list, cost) in products {
        s.products.insert(
            id.into(),
            Product {
                id: id.into(),
                name: name.into(),
                family: family.into(),
                unit: "each".into(),
                currency: Currency::USD,
                list_price: Money(list),
            },
        );
        s.costs.push(Cost {
            product_id: id.into(),
            version: 1,
            effective_from: Date::parse("2026-01-01")?,
            amount: Money(cost),
            currency: Currency::USD,
            unit: "each".into(),
        });
    }
    for (id, name, segment) in [
        ("C-ATLAS", "Atlas Process Systems (synthetic)", "OEM"),
        ("C-NORTH", "Northline Service (synthetic)", "Service"),
        ("C-DELTA", "Delta Research Labs (synthetic)", "Research"),
    ] {
        s.customers.insert(
            id.into(),
            Customer {
                id: id.into(),
                name: name.into(),
                segment: segment.into(),
            },
        );
    }
    s.rules.push(Rule {
        id: "OEM-VOLUME".into(),
        version: 1,
        name: "OEM volume tier".into(),
        priority: 10,
        product_id: None,
        customer_id: None,
        segment: Some("OEM".into()),
        effective_from: Date::parse("2026-01-01")?,
        effective_to: None,
        min_quantity: Quantity(20000),
        action: RuleAction::Discount(Bps(800)),
        currency: Currency::USD,
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn actor(name: &str, role: Role) -> Actor {
        Actor {
            name: name.into(),
            role,
        }
    }
    fn demo() -> Result<Engine> {
        let mut e = Engine::memory();
        e.run(actor("admin", Role::Admin), Command::SeedDemo)?;
        Ok(e)
    }
    fn request() -> Result<PriceRequest> {
        Ok(PriceRequest {
            source: None,
            conversion: None,
            product_id: "VLV-100".into(),
            customer_id: "C-ATLAS".into(),
            date: Date::parse("2026-10-13")?,
            quantity: Quantity(10000),
            discount: Bps(1000),
            rebate: Bps(500),
        })
    }
    fn add_quote(e: &mut Engine) -> Result<()> {
        e.run(
            actor("seller", Role::Sales),
            Command::QuoteCreate {
                id: "Q-1".into(),
                title: "Synthetic quote".into(),
                customer_id: "C-ATLAS".into(),
                date: Date::parse("2026-10-13")?,
            },
        )?;
        e.run(
            actor("seller", Role::Sales),
            Command::QuoteAddLine {
                id: "Q-1".into(),
                version: 1,
                request: request()?,
            },
        )?;
        Ok(())
    }
    #[test]
    fn approval_lifecycle_and_snapshot_immutability() -> Result<()> {
        let mut e = demo()?;
        add_quote(&mut e)?;
        e.run(
            actor("seller", Role::Sales),
            Command::QuoteSubmit {
                id: "Q-1".into(),
                version: 2,
            },
        )?;
        let before = e.state.clone();
        assert!(
            e.run(
                actor("seller", Role::Admin),
                Command::QuoteDecide {
                    id: "Q-1".into(),
                    version: 3,
                    approve: true,
                    note: "Self decision".into()
                }
            )
            .is_err()
        );
        assert_eq!(e.state, before);
        e.run(
            actor("approver", Role::Approver),
            Command::QuoteDecide {
                id: "Q-1".into(),
                version: 3,
                approve: true,
                note: "Commercial review complete".into(),
            },
        )?;
        let q = e
            .state
            .quotes
            .get("Q-1")
            .ok_or_else(|| error("Missing quote"))?
            .clone();
        let csv = export_quote(&q)?;
        assert!(csv.contains("855.00"));
        let mut product = e
            .state
            .products
            .get("VLV-100")
            .ok_or_else(|| error("Missing product"))?
            .clone();
        product.list_price = Money(20000);
        e.run(
            actor("analyst", Role::Analyst),
            Command::ProductPut(product),
        )?;
        assert_eq!(e.state.quotes.get("Q-1"), Some(&q));
        assert_eq!(export_quote(&q)?, csv);
        assert!(
            e.run(
                actor("seller", Role::Sales),
                Command::QuoteAddLine {
                    id: "Q-1".into(),
                    version: 4,
                    request: request()?
                }
            )
            .is_err()
        );
        e.run(
            actor("seller", Role::Sales),
            Command::QuoteRevise {
                id: "Q-1".into(),
                version: 4,
            },
        )?;
        let revised = e
            .state
            .quotes
            .get("Q-1")
            .ok_or_else(|| error("Missing quote"))?;
        assert_eq!(revised.status, Status::Draft);
        assert!(export_quote(revised).is_err());
        Ok(())
    }
    #[test]
    fn role_conflict_and_idempotency() -> Result<()> {
        let mut e = Engine::memory();
        let r = Request {
            request_id: "req-1".into(),
            expected_revision: 0,
            actor: actor("admin", Role::Admin),
            action: Command::SeedDemo,
        };
        let encoded = serde_json::to_string(&r).map_err(|e| error(e.to_string()))?;
        let parsed: Request = serde_json::from_str(&encoded).map_err(|e| error(e.to_string()))?;
        e.execute(parsed)?;
        assert!(e.execute(r.clone())?.replayed);
        assert_eq!(e.state.revision, 1);
        let mut changed = r;
        changed.actor.name = "someone-else".into();
        assert!(e.execute(changed).is_err());
        assert!(
            e.run(
                actor("sales", Role::Sales),
                Command::PolicySet(Policy::default())
            )
            .is_err()
        );
        let mut stale = Request {
            request_id: "stale".into(),
            expected_revision: 0,
            actor: actor("admin", Role::Admin),
            action: Command::PolicySet(Policy::default()),
        };
        assert!(e.execute(stale.clone()).is_err());
        stale.expected_revision = 1;
        e.execute(stale)?;
        Ok(())
    }
    #[test]
    fn atomic_import_and_invalid_input_rollback() -> Result<()> {
        let mut e = demo()?;
        let before = e.state.clone();
        assert!(e.run(actor("analyst",Role::Analyst),Command::ProductsImport{csv:"id,name,family,unit,currency,list_price\nNEW,New valve,Flow,each,USD,50.00\nVLV-100,Duplicate,Flow,each,USD,5.00".into()}).is_err());
        assert_eq!(before, e.state);
        let mut r = request()?;
        r.discount = Bps(10001);
        assert!(evaluate(e.state(), &r).is_err());
        assert!(Date::parse("2026-02-30").is_err());
        Ok(())
    }
    #[test]
    fn whole_list_approval_and_publish() -> Result<()> {
        let mut e = demo()?;
        e.run(
            actor("analyst", Role::Analyst),
            Command::ListCalculate {
                id: "L-1".into(),
                name: "October OEM".into(),
                customer_id: "C-ATLAS".into(),
                date: Date::parse("2026-10-13")?,
                quantity: Quantity(1000),
                currency: Currency::USD,
            },
        )?;
        assert!(
            e.run(
                actor("analyst", Role::Analyst),
                Command::ListPublish {
                    id: "L-1".into(),
                    version: 1
                }
            )
            .is_err()
        );
        e.run(
            actor("analyst", Role::Analyst),
            Command::ListSubmit {
                id: "L-1".into(),
                version: 1,
            },
        )?;
        e.run(
            actor("approver", Role::Approver),
            Command::ListDecide {
                id: "L-1".into(),
                version: 2,
                approve: true,
                note: "Approved for demo".into(),
            },
        )?;
        e.run(
            actor("analyst", Role::Analyst),
            Command::ListPublish {
                id: "L-1".into(),
                version: 3,
            },
        )?;
        assert_eq!(
            e.state.price_lists.get("L-1").map(|l| l.status),
            Some(Status::Published)
        );
        Ok(())
    }
    #[test]
    fn persistence_e2e() -> Result<()> {
        let dir = std::env::temp_dir().join(format!("copperline-engine-{}", std::process::id()));
        if dir.exists() {
            std::fs::remove_dir_all(&dir).map_err(|e| error(e.to_string()))?;
        }
        let mut e = Engine::open(&dir)?;
        e.run(actor("admin", Role::Admin), Command::SeedDemo)?;
        add_quote(&mut e)?;
        let expected = e.state.clone();
        drop(e);
        let e = Engine::open(&dir)?;
        assert_eq!(e.state, expected);
        drop(e);
        std::fs::remove_dir_all(dir).map_err(|e| error(e.to_string()))?;
        Ok(())
    }
}
