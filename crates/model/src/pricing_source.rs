//! Explicit commercial price selection and immutable evidence for frozen documents.
use super::*;

/// Missing source means the live catalog/rule engine. An explicit source never falls back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", deny_unknown_fields)]
pub enum PriceSource {
    Grid { id: String, approval_revision: u64 },
    PriceList { id: String, version: u64 },
    Contract { id: String, edition: u64 },
    CustomerContract,
}
impl PriceSource {
    pub fn label(&self) -> String {
        match self {
            Self::Grid {
                id,
                approval_revision,
            } => format!("Grid {id} · approval {approval_revision}"),
            Self::PriceList { id, version } => format!("List {id} · v{version}"),
            Self::Contract { id, edition } => format!("Contract {id} · edition {edition}"),
            Self::CustomerContract => "Automatic customer contract (highest priority)".into(),
        }
    }
    pub fn validate(&self) -> Result<()> {
        let (key, version) = match self {
            Self::Grid {
                id,
                approval_revision,
            } => (id, approval_revision),
            Self::PriceList { id, version } => (id, version),
            Self::Contract { id, edition } => (id, edition),
            Self::CustomerContract => return Ok(()),
        };
        id(key)?;
        ensure(
            *version > 0,
            "Pricing source requires a positive version/revision",
        )
    }
}

/// The approved invoice unit becomes the new negotiated discount base. Source
/// discounts are already included; source rebates are never carried forward.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceEvidence {
    pub source: PriceSource,
    /// Legacy M2 grid records did not store a document version; approval revision
    /// remains their unique immutable version identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_version: Option<u64>,
    pub pricing_revision: u64,
    pub approval_revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publication_revision: Option<u64>,
    pub captured_revision: u64,
    pub product_id: String,
    pub customer_id: String,
    pub currency: Currency,
    pub unit: String,
    /// Evaluated pricing date. Legacy grid/list sources require this exact date;
    /// optional contract evidence supplies the approved interval and quantity band.
    pub effective_date: Date,
    pub min_quantity: Quantity,
    pub invoice_unit: Money,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contract: Option<ContractSourceEvidence>,
}
impl SourceEvidence {
    fn validate_for(&self, evaluation: &Evaluation) -> Result<()> {
        self.source.validate()?;
        ensure(
            match &self.source {
                PriceSource::Grid {
                    approval_revision, ..
                } => {
                    *approval_revision == self.approval_revision
                        && self.publication_revision.is_none()
                        && self.contract.is_none()
                }
                PriceSource::PriceList { version, .. } => {
                    self.document_version == Some(*version)
                        && self.publication_revision.is_some()
                        && self.contract.is_none()
                }
                PriceSource::Contract { edition, .. } => {
                    self.document_version == Some(*edition)
                        && self.publication_revision.is_none()
                        && self.contract.is_some()
                }
                PriceSource::CustomerContract => false,
            },
            "Source identity does not match approval/publication evidence",
        )?;
        id(&self.product_id)?;
        id(&self.customer_id)?;
        id(&self.unit)?;
        self.effective_date.validate()?;
        self.min_quantity.validate()?;
        ensure(
            self.invoice_unit.0 >= 0,
            "Approved source unit must be nonnegative",
        )?;
        ensure(
            self.document_version.is_none_or(|v| v > 0)
                && self.pricing_revision < self.approval_revision
                && self.approval_revision <= self.captured_revision
                && self
                    .publication_revision
                    .is_none_or(|r| r > self.approval_revision && r <= self.captured_revision),
            "Pricing source revision evidence is invalid",
        )?;
        if let Some(contract) = &self.contract {
            id(&contract.row_id)?;
            contract.start.validate()?;
            contract.end.validate()?;
            ensure(
                contract.start <= evaluation.request.date && evaluation.request.date < contract.end,
                "Frozen contract date is outside its half-open effective interval",
            )?;
            if let Some(max) = contract.max_quantity {
                max.validate()?;
                ensure(
                    max > self.min_quantity && evaluation.request.quantity < max,
                    "Frozen contract quantity is outside its half-open band",
                )?;
            }
            ensure(
                contract.terms_fingerprint.len() == 64
                    && contract
                        .terms_fingerprint
                        .bytes()
                        .all(|c| c.is_ascii_hexdigit()),
                "Contract terms fingerprint invalid",
            )?;
        }
        ensure(
            (evaluation.request.source.as_ref() == Some(&self.source)
                || (evaluation.request.source == Some(PriceSource::CustomerContract)
                    && matches!(self.source, PriceSource::Contract { .. })))
                && evaluation.request.product_id == self.product_id
                && evaluation.request.customer_id == self.customer_id
                && evaluation.currency == self.currency
                && evaluation.unit == self.unit
                && evaluation.request.date == self.effective_date
                && evaluation.request.quantity >= self.min_quantity
                && evaluation.rule_unit == self.invoice_unit
                && evaluation.rule.is_none(),
            "Frozen pricing source context or discount base mismatch",
        )
    }
}

impl Evaluation {
    pub(crate) fn validate_pricing_source_snapshot(&self) -> Result<()> {
        match (&self.request.source, &self.source) {
            (None, None) => Ok(()),
            (Some(_), Some(evidence)) => evidence.validate_for(self),
            _ => Err(error(
                "Explicit pricing source requires matching frozen evidence",
            )),
        }
    }
}

impl State {
    /// Resolve only currently active, explicitly identified commercial prices.
    /// Old grid prices remain usable as historical evidence, never new selections.
    pub fn resolve_pricing_source(&self, request: &PriceRequest) -> Result<Option<SourceEvidence>> {
        self.pricing_source_at(request, self.revision, false)
    }
    pub(crate) fn pricing_source_at(
        &self,
        request: &PriceRequest,
        revision: u64,
        historical: bool,
    ) -> Result<Option<SourceEvidence>> {
        let Some(source) = &request.source else {
            return Ok(None);
        };
        source.validate()?;
        request.date.validate()?;
        request.quantity.validate()?;
        let evidence = if matches!(
            source,
            PriceSource::Contract { .. } | PriceSource::CustomerContract
        ) {
            self.contract_source_evidence(request, revision)?
        } else {
            self.source_evidence(source, &request.product_id, revision, historical)?
        };
        ensure(
            evidence.product_id == request.product_id
                && evidence.customer_id == request.customer_id,
            "Approved pricing source does not match this product/customer",
        )?;
        ensure(
            evidence.effective_date == request.date,
            "Approved source is valid only on its exact pricing date; choose that date or use live rules",
        )?;
        ensure(
            request.quantity >= evidence.min_quantity,
            "Quantity is below the approved source minimum (its calculated quantity)",
        )?;
        let product = self
            .products
            .get(&request.product_id)
            .ok_or_else(|| error("Product not found"))?;
        ensure(
            evidence.currency == product.currency && evidence.unit == product.unit,
            "Approved source currency/unit mismatch; no implicit FX or UOM conversion",
        )?;
        Ok(Some(evidence))
    }

    fn source_evidence(
        &self,
        source: &PriceSource,
        product_id: &str,
        captured_revision: u64,
        historical: bool,
    ) -> Result<SourceEvidence> {
        let (line, document_version, pricing_revision, decision, publication_revision) =
            match source {
                PriceSource::Grid {
                    id,
                    approval_revision,
                } => {
                    let grid = self
                        .grid
                        .get(id)
                        .ok_or_else(|| error("Selected grid item does not exist"))?;
                    let active = grid
                        .last_approved
                        .as_ref()
                        .filter(|p| p.decision.revision == *approval_revision);
                    let selected = if historical {
                        active.or_else(|| grid.approved_history.get(approval_revision))
                    } else {
                        active
                    };
                    let approved = selected.ok_or_else(|| error("Selected grid approval is unavailable or stale; select the current active approval"))?;
                    ensure(approved.decision.approved, "Grid source is not approved")?;
                    ensure(
                        grid.decisions.contains(&approved.decision)
                            && !grid.decisions.iter().any(|d| {
                                d.approved
                                    && d.revision > *approval_revision
                                    && d.revision <= captured_revision
                            }),
                        "Selected grid approval was not active at the recorded capture revision",
                    )?;
                    (
                        &approved.evaluation,
                        approved.document_version,
                        approved.source_revision,
                        &approved.decision,
                        None,
                    )
                }
                PriceSource::PriceList { id, version } => {
                    let list = self
                        .price_lists
                        .get(id)
                        .ok_or_else(|| error("Selected price list does not exist"))?;
                    ensure(
                        list.status == Status::Published && list.version == *version,
                        "Select the exact published price list version",
                    )?;
                    let mut matching = list
                        .lines
                        .iter()
                        .filter(|l| l.request.product_id == product_id);
                    let line = matching
                        .next()
                        .ok_or_else(|| error("Product missing from selected price list"))?;
                    ensure(
                        matching.next().is_none(),
                        "Selected price list has ambiguous product lines",
                    )?;
                    let decision = list
                        .decisions
                        .last()
                        .filter(|d| d.approved)
                        .ok_or_else(|| error("Published list approval missing"))?;
                    let detail = format!("Published list {id} locally");
                    let mut publications = self
                        .audit
                        .iter()
                        .filter(|a| a.command == "list_publish" && a.detail == detail);
                    let publication = publications
                        .next()
                        .ok_or_else(|| error("Published list audit missing"))?;
                    ensure(
                        publications.next().is_none(),
                        "Ambiguous list publication audit",
                    )?;
                    ensure(
                        matches!(publication.actor.role, Role::Analyst | Role::Admin)
                            && publication
                                .target
                                .as_deref()
                                .is_none_or(|t| t == format!("list:{id}")),
                        "Invalid list publication actor",
                    )?;
                    (
                        line,
                        Some(*version),
                        list.source_revision,
                        decision,
                        Some(publication.revision),
                    )
                }
                PriceSource::Contract { .. } | PriceSource::CustomerContract => {
                    return Err(error("Contract source requires interval resolution"));
                }
            };
        ensure(
            line.request.source.is_none() && line.source.is_none(),
            "Chained approved pricing sources are not supported",
        )?;
        line.validate_snapshot()?;
        let (command, target) = match source {
            PriceSource::Grid { id, .. } => ("grid_decide", format!("grid:{id}")),
            PriceSource::PriceList { id, .. } => ("list_decide", format!("list:{id}")),
            PriceSource::Contract { .. } | PriceSource::CustomerContract => {
                return Err(error("Unexpected legacy source kind"));
            }
        };
        self.validate_decision_audit(std::slice::from_ref(decision), command, &target)?;
        ensure(
            !line.hard_block,
            "Approved pricing source violates its hard guardrail",
        )?;
        ensure(
            pricing_revision < decision.revision
                && decision.revision <= captured_revision
                && publication_revision
                    .is_none_or(|r| r > decision.revision && r <= captured_revision)
                && captured_revision <= self.revision,
            "Pricing source approval/publication occurred after capture",
        )?;
        Ok(SourceEvidence {
            source: source.clone(),
            document_version,
            pricing_revision,
            approval_revision: decision.revision,
            publication_revision,
            captured_revision,
            product_id: line.request.product_id.clone(),
            customer_id: line.request.customer_id.clone(),
            currency: line.currency,
            unit: line.unit.clone(),
            effective_date: line.request.date.clone(),
            min_quantity: line.request.quantity,
            invoice_unit: line.invoice_unit,
            contract: None,
        })
    }

    pub(crate) fn validate_pricing_source_reference(
        &self,
        line: &Evaluation,
        source_revision: u64,
    ) -> Result<()> {
        line.validate_pricing_source_snapshot()?;
        if let Some(evidence) = &line.source {
            ensure(
                evidence.captured_revision <= source_revision,
                "Pricing source evidence postdates this document's calculation",
            )?;
            let expected = self
                .pricing_source_at(&line.request, evidence.captured_revision, true)?
                .ok_or_else(|| error("Frozen source selector missing"))?;
            ensure(
                *evidence == expected,
                "Frozen source evidence differs from approved source history",
            )?;
        }
        Ok(())
    }

    pub(crate) fn validate_grid_source_history(&self, item: &GridItem) -> Result<()> {
        ensure(
            item.approved_history.len() <= 10_000,
            "Grid approval history limit reached",
        )?;
        let active_revision = item.last_approved.as_ref().map(|a| a.decision.revision);
        for (revision, approved) in &item.approved_history {
            ensure(
                Some(*revision) != active_revision
                    && *revision == approved.decision.revision
                    && active_revision.is_some_and(|active| *revision < active)
                    && item.decisions.contains(&approved.decision),
                "Archived grid approval is not an earlier recorded decision",
            )?;
            self.validate_approved_source_record(item, approved)?;
        }
        if let Some(approved) = &item.last_approved {
            self.validate_approved_source_record(item, approved)?;
            ensure(
                approved.document_version.is_none_or(|v| {
                    if item.status == Status::Approved {
                        v == item.version
                    } else {
                        v < item.version
                    }
                }),
                "Active grid document version does not match its lifecycle",
            )?;
        }
        Ok(())
    }

    fn validate_approved_source_record(
        &self,
        item: &GridItem,
        approved: &ApprovedPrice,
    ) -> Result<()> {
        ensure(
            approved
                .document_version
                .is_none_or(|v| v > 0 && v <= item.version)
                && approved.decision.approved
                && approved.decision.revision > approved.source_revision
                && approved.decision.revision <= self.revision
                && approved.evaluation.request.product_id == item.candidate.request.product_id
                && approved.evaluation.request.customer_id == item.candidate.request.customer_id
                && !approved.evaluation.hard_block,
            "Archived grid price identity/version/approval mismatch",
        )?;
        approved.policy.validate()?;
        approved.evaluation.validate_snapshot()?;
        approved.evaluation.check_policy(&approved.policy)?;
        self.validate_snapshot_references(&approved.evaluation, approved.source_revision)?;
        self.validate_decision_audit(
            std::slice::from_ref(&approved.decision),
            "grid_decide",
            &format!("grid:{}", item.id),
        )
    }
}
