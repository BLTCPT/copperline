use super::*;

pub(super) struct CreditForm {
    pub(super) invoice: String,
    loaded: String,
    pub(super) id: String,
    pub(super) date: String,
    pub(super) quantity: String,
    pub(super) reason: String,
    pub(super) selected: String,
    pub(super) export_path: String,
    prepare_open: Option<bool>,
}
impl Default for CreditForm {
    fn default() -> Self {
        Self {
            invoice: String::new(),
            loaded: String::new(),
            id: String::new(),
            date: String::new(),
            quantity: "1".into(),
            reason: "Synthetic quantity credit".into(),
            selected: String::new(),
            export_path: "approved-credit.csv".into(),
            prepare_open: None,
        }
    }
}
impl Workbench {
    pub(super) fn open_quantity_credit(&mut self, invoice: &str) {
        self.credit_form.invoice = invoice.into();
        self.credit_form.loaded.clear();
        self.credit_form.id.clear();
        self.credit_form.prepare_open = Some(true);
        self.view = View::Credits;
    }

    pub(super) fn credits_view(&mut self, ui: &mut egui::Ui) {
        ui.label("Credit a positive quantity in the invoice's recorded unit. Amounts come from the immutable original totals and cumulative rounding; they cannot be entered manually.");
        ui.small("Approval restates the original invoice and rebate period. The record date does not schedule activation. Closed-period credits and next-period carry-forward are unsupported; no payment, inventory or accounting posting is made.");
        egui::CollapsingHeader::new("Prepare a quantity credit")
            .id_salt("credit-prepare")
            .default_open(self.engine.state().credits.is_empty())
            .open(self.credit_form.prepare_open.take())
            .show(ui, |ui| self.prepare_credit(ui));
        ui.separator();
        ui.heading("Credit review and evidence");
        let records: Vec<_> = self.engine.state().credits.values().cloned().collect();
        if !records
            .iter()
            .any(|c| c.proposal.id == self.credit_form.selected)
        {
            self.credit_form.selected = records
                .iter()
                .max_by_key(|c| c.created_revision)
                .map(|c| c.proposal.id.clone())
                .unwrap_or_default();
        }
        egui::ComboBox::from_id_salt("credit-record")
            .selected_text(&self.credit_form.selected)
            .show_ui(ui, |ui| {
                for c in &records {
                    ui.selectable_value(
                        &mut self.credit_form.selected,
                        c.proposal.id.clone(),
                        format!(
                            "{} · {:?} · {}",
                            c.proposal.id, c.status, c.proposal.original_transaction_id
                        ),
                    );
                }
            });
        if let Some(c) = records
            .iter()
            .find(|c| c.proposal.id == self.credit_form.selected)
        {
            ui.label(format!(
                "{} · {:?} · v{} · owner {} · created r{}",
                c.proposal.id, c.status, c.version, c.owner, c.created_revision
            ));
            ui.label(format!(
                "Original invoice {} · {} · {} · {} · source {}",
                c.original.id,
                c.original.customer_id,
                c.original.product_id,
                c.original.date,
                c.original.source
            ));
            Self::credit_amounts(ui, c);
            ui.label(format!(
                "Record date {} · {}",
                c.proposal.effective_date, c.proposal.reason
            ));
            if let Some(review) = &c.review {
                Self::record_review(ui, review);
            }
            let current = self
                .engine
                .state()
                .ensure_credit_current(c, self.engine.state().revision);
            if matches!(c.status, RecordStatus::Draft | RecordStatus::Pending)
                && let Err(e) = &current
            {
                ui.colored_label(COPPER, e.to_string());
            }
            field(ui, "Decision note", &mut self.note);
            let actor = self.actor();
            let owner = c.owner == actor.name && matches!(actor.role, Role::Analyst | Role::Admin);
            let reviewer = actor.role == Role::Finance && !c.contributors.contains(&actor.name);
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(
                        c.status == RecordStatus::Draft && owner && current.is_ok(),
                        egui::Button::new("Submit credit"),
                    )
                    .clicked()
                {
                    self.run(Command::CreditSubmit {
                        id: c.proposal.id.clone(),
                        version: c.version,
                    });
                }
                if ui
                    .add_enabled(
                        c.status == RecordStatus::Pending && reviewer && current.is_ok(),
                        egui::Button::new("Approve credit"),
                    )
                    .clicked()
                {
                    self.run(Command::CreditDecide {
                        id: c.proposal.id.clone(),
                        version: c.version,
                        approve: true,
                        note: self.note.clone(),
                    });
                }
                if ui
                    .add_enabled(
                        c.status == RecordStatus::Pending && reviewer,
                        egui::Button::new("Reject credit"),
                    )
                    .clicked()
                {
                    self.run(Command::CreditDecide {
                        id: c.proposal.id.clone(),
                        version: c.version,
                        approve: false,
                        note: self.note.clone(),
                    });
                }
                if ui
                    .add_enabled(
                        matches!(c.status, RecordStatus::Draft | RecordStatus::Pending) && owner,
                        egui::Button::new("Withdraw credit"),
                    )
                    .clicked()
                {
                    self.run(Command::CreditWithdraw {
                        id: c.proposal.id.clone(),
                        version: c.version,
                    });
                }
                if ui.button("Prepare another quantity credit").clicked() {
                    self.open_quantity_credit(&c.proposal.original_transaction_id);
                }
            });
            ui.small("Only the owner submits or withdraws; independent Finance decides. Original authors, booking/quote contributors and prior credit authors cannot approve.");
            egui::CollapsingHeader::new("Frozen credit provenance").show(ui, |ui| {
                ui.label(format!(
                    "Prior approved credits: {}",
                    if c.prior_credit_ids.is_empty() {
                        "None".into()
                    } else {
                        c.prior_credit_ids.join(", ")
                    }
                ));
                ui.label(format!(
                    "Excluded contributors: {}",
                    c.contributors
                        .iter()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
                match serde_json::to_string_pretty(c) {
                    Ok(json) => {
                        ui.monospace(json);
                    }
                    Err(e) => {
                        ui.colored_label(Color32::DARK_RED, e.to_string());
                    }
                }
            });
            field(ui, "Credit CSV file", &mut self.credit_form.export_path);
            if ui
                .add_enabled(
                    c.status == RecordStatus::Approved,
                    egui::Button::new("Export credit evidence"),
                )
                .clicked()
            {
                let result = conversions::write_export(
                    &self.credit_form.export_path,
                    copperline_engine::export_credit(self.engine.state(), &c.proposal.id),
                );
                self.result(
                    result,
                    "Frozen credit evidence exported; no payment or posting.",
                );
            }
        } else {
            ui.label("No credit proposals yet.");
        }
    }

    fn prepare_credit(&mut self, ui: &mut egui::Ui) {
        let invoices: Vec<_> = self.engine.state().transactions.values().cloned().collect();
        if !invoices.iter().any(|t| t.id == self.credit_form.invoice) {
            self.credit_form.invoice = invoices.first().map(|t| t.id.clone()).unwrap_or_default();
        }
        egui::ComboBox::from_id_salt("credit-invoice")
            .selected_text(&self.credit_form.invoice)
            .show_ui(ui, |ui| {
                for t in &invoices {
                    ui.selectable_value(
                        &mut self.credit_form.invoice,
                        t.id.clone(),
                        format!(
                            "{} · {} {} · {} {}",
                            t.id, t.currency, t.invoice_total, t.quantity, t.unit
                        ),
                    );
                }
            });
        let Some(original) = invoices.iter().find(|t| t.id == self.credit_form.invoice) else {
            ui.label("Record or book an invoice first.");
            return;
        };
        if self.credit_form.loaded != original.id {
            self.credit_form.loaded = original.id.clone();
            self.credit_form.date = original.date.to_string();
            if self.credit_form.id.is_empty() {
                self.credit_form.id =
                    format!("CN-{:04}", self.engine.state().revision.saturating_add(1));
            }
        }
        ui.label(format!(
            "Original {} · {} · {} · {} · {} {}",
            original.id,
            original.customer_id,
            original.product_id,
            original.date,
            original.quantity,
            original.unit
        ));
        field(ui, "New credit ID", &mut self.credit_form.id);
        field(ui, "Credit quantity", &mut self.credit_form.quantity);
        ui.small(format!("Quantity is in recorded {}. Positive quantities may allocate zero cents; final exhaustion clears every original cent.", original.unit));
        field(ui, "Credit record date", &mut self.credit_form.date);
        field(ui, "Credit reason", &mut self.credit_form.reason);
        let preview = (|| -> Result<Credit> {
            let proposal = CreditProposal {
                id: self.credit_form.id.clone(),
                original_transaction_id: original.id.clone(),
                effective_date: Date::parse(&self.credit_form.date)?,
                reason: self.credit_form.reason.clone(),
                quantity: self.credit_form.quantity.parse()?,
            };
            self.engine
                .state()
                .credit_preview(&proposal, &self.actor().name)
        })();
        match &preview {
            Ok(c) => Self::credit_amounts(ui, c),
            Err(e) => {
                ui.colored_label(COPPER, format!("Credit unavailable: {e}"));
            }
        }
        let permitted = matches!(self.actor().role, Role::Analyst | Role::Admin);
        if ui
            .add_enabled(
                permitted && preview.is_ok(),
                egui::Button::new("Create credit proposal"),
            )
            .clicked()
            && let Ok(c) = preview
        {
            let id = c.proposal.id.clone();
            self.run(Command::CreditCreate {
                proposal: c.proposal,
            });
            if !self.failed {
                self.credit_form.selected = id;
                self.credit_form.prepare_open = Some(false);
            }
        }
        ui.small("A live/full reversal and a quantity credit cannot share an invoice. No credit reversal, value-only credit or replacement is supported.");
    }

    fn credit_amounts(ui: &mut egui::Ui, c: &Credit) {
        ui.strong(format!(
            "Credit {} {} invoice / {} cost · {} {}",
            c.original.currency, c.invoice, c.cost, c.proposal.quantity, c.original.unit
        ));
        ui.label(format!(
            "Original {} {} · prior approved quantity {} {} · requested {} {}",
            c.original.quantity,
            c.original.unit,
            c.prior_quantity,
            c.original.unit,
            c.proposal.quantity,
            c.original.unit
        ));
        if let Some(b) = &c.booking {
            ui.label(format!(
                "Booking {} · line {} · nonposted native credit {} {} / {} {} invoice / {} cost",
                b.booking_id,
                b.line_index.saturating_add(1),
                b.native_quantity,
                b.native_basis.unit,
                b.native_basis.currency,
                b.native_invoice,
                b.native_cost
            ));
        }
        ui.small("Invoice and cost use cumulative allocation from the original totals. Reporting allocates from the converted whole original, preserving its historical FX cutoff.");
    }

    pub(super) fn record_review(ui: &mut egui::Ui, review: &RecordReview) {
        ui.label(format!(
            "{} by {} ({:?}) at r{} · {}",
            if review.approved {
                "Approved"
            } else {
                "Rejected"
            },
            review.actor.name,
            review.actor.role,
            review.revision,
            review.note
        ));
    }
}
