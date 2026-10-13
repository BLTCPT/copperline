use super::*;

pub(super) struct PeriodCloseForm {
    pub(super) id: String,
    pub(super) start: String,
    pub(super) end: String,
    pub(super) currency: Currency,
    pub(super) reason: String,
    pub(super) selected: String,
    pub(super) export_path: String,
    prepare_open: Option<bool>,
}
impl Default for PeriodCloseForm {
    fn default() -> Self {
        Self {
            id: String::new(),
            start: "2026-10-01".into(),
            end: "2026-11-01".into(),
            currency: Currency::USD,
            reason: "Review synthetic nominal invoice period".into(),
            selected: String::new(),
            export_path: "approved-period-close.csv".into(),
            prepare_open: None,
        }
    }
}
impl Workbench {
    pub(super) fn period_closes_view(&mut self, ui: &mut egui::Ui) {
        ui.label("Freeze the nominal invoice basis for one recorded currency and original invoice-date interval, across all customers. This is a local commercial control, not an accounting period, payment, tax or inventory close.");
        ui.colored_label(COPPER, "Approved closes block invoice ingestion, quote bookings, credits and full corrections affecting the interval. There is no reopen or next-period carry-forward; a later credit record date does not bypass the original closed period.");
        ui.small("Rebate accruals, settlement adjustments and reporting FX calculations are outside this nominal close. Existing frozen evidence remains unchanged.");
        if self.period_close_form.id.is_empty() {
            self.period_close_form.id =
                format!("PC-{:04}", self.engine.state().revision.saturating_add(1));
        }
        egui::CollapsingHeader::new("Prepare an invoice period close")
            .id_salt("period-close-prepare")
            .default_open(self.engine.state().period_closes.is_empty())
            .open(self.period_close_form.prepare_open.take())
            .show(ui, |ui| self.prepare_period_close(ui));
        ui.separator();
        ui.heading("Nominal period close records");
        let records: Vec<_> = self
            .engine
            .state()
            .period_closes
            .values()
            .cloned()
            .collect();
        if !records
            .iter()
            .any(|c| c.terms.id == self.period_close_form.selected)
        {
            self.period_close_form.selected = records
                .iter()
                .max_by_key(|c| c.created_revision)
                .map(|c| c.terms.id.clone())
                .unwrap_or_default();
        }
        egui::ComboBox::from_id_salt("period-close-record")
            .selected_text(&self.period_close_form.selected)
            .show_ui(ui, |ui| {
                for c in &records {
                    ui.selectable_value(
                        &mut self.period_close_form.selected,
                        c.terms.id.clone(),
                        format!(
                            "{} · {:?} · {} [{} , {})",
                            c.terms.id, c.status, c.terms.currency, c.terms.start, c.terms.end
                        ),
                    );
                }
            });
        if let Some(c) = records
            .iter()
            .find(|c| c.terms.id == self.period_close_form.selected)
        {
            ui.label(format!(
                "{} · {:?} · v{} · owner {} · created r{} · source r{}",
                c.terms.id, c.status, c.version, c.owner, c.created_revision, c.source_revision
            ));
            ui.label(format!(
                "{} · [{} , {}) · {}",
                c.terms.currency, c.terms.start, c.terms.end, c.terms.reason
            ));
            Self::period_close_basis(ui, c);
            if let Some(review) = &c.review {
                Self::record_review(ui, review);
            }
            let current = self
                .engine
                .state()
                .ensure_period_close_current(c, self.engine.state().revision);
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
                        egui::Button::new("Submit period close"),
                    )
                    .clicked()
                {
                    self.run(Command::PeriodCloseSubmit {
                        id: c.terms.id.clone(),
                        version: c.version,
                    });
                }
                if ui
                    .add_enabled(
                        c.status == RecordStatus::Pending && reviewer && current.is_ok(),
                        egui::Button::new("Approve period close"),
                    )
                    .clicked()
                {
                    self.run(Command::PeriodCloseDecide {
                        id: c.terms.id.clone(),
                        version: c.version,
                        approve: true,
                        note: self.note.clone(),
                    });
                }
                if ui
                    .add_enabled(
                        c.status == RecordStatus::Pending && reviewer,
                        egui::Button::new("Reject period close"),
                    )
                    .clicked()
                {
                    self.run(Command::PeriodCloseDecide {
                        id: c.terms.id.clone(),
                        version: c.version,
                        approve: false,
                        note: self.note.clone(),
                    });
                }
                if ui
                    .add_enabled(
                        matches!(c.status, RecordStatus::Draft | RecordStatus::Pending) && owner,
                        egui::Button::new("Withdraw period close"),
                    )
                    .clicked()
                {
                    self.run(Command::PeriodCloseWithdraw {
                        id: c.terms.id.clone(),
                        version: c.version,
                    });
                }
                if ui.button("Prepare another period close").clicked() {
                    self.period_close_form.id.clear();
                    self.period_close_form.prepare_open = Some(true);
                }
            });
            ui.small("Owner submits or withdraws; independent Finance decides. All source authors are excluded. Submission and approval require unchanged exact invoice, reversal and credit IDs and no affected unresolved proposals.");
            egui::CollapsingHeader::new("Frozen close provenance").show(ui, |ui| {
                for (label, ids) in [
                    ("Invoice IDs", &c.basis.transaction_ids),
                    ("Full reversal IDs", &c.basis.correction_ids),
                    ("Credit IDs", &c.basis.credit_ids),
                ] {
                    ui.label(format!(
                        "{label}: {}",
                        if ids.is_empty() {
                            "None".into()
                        } else {
                            ids.join(", ")
                        }
                    ));
                }
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
            field(
                ui,
                "Period close CSV file",
                &mut self.period_close_form.export_path,
            );
            if ui
                .add_enabled(
                    c.status == RecordStatus::Approved,
                    egui::Button::new("Export period close evidence"),
                )
                .clicked()
            {
                let result = conversions::write_export(
                    &self.period_close_form.export_path,
                    copperline_engine::export_period_close(self.engine.state(), &c.terms.id),
                );
                self.result(
                    result,
                    "Frozen nominal period close exported; no accounting posting.",
                );
            }
        } else {
            ui.label("No nominal period close proposals yet.");
        }
    }

    fn prepare_period_close(&mut self, ui: &mut egui::Ui) {
        field(ui, "New close ID", &mut self.period_close_form.id);
        ui.horizontal_wrapped(|ui| {
            field(
                ui,
                "Original invoices from",
                &mut self.period_close_form.start,
            );
            field(ui, "Until (exclusive)", &mut self.period_close_form.end);
        });
        ui.horizontal(|ui| {
            ui.label("Recorded currency");
            egui::ComboBox::from_id_salt("close-currency")
                .selected_text(self.period_close_form.currency.to_string())
                .show_ui(ui, |ui| {
                    for c in [Currency::USD, Currency::EUR, Currency::GBP] {
                        ui.selectable_value(&mut self.period_close_form.currency, c, c.to_string());
                    }
                });
        });
        field(ui, "Close reason", &mut self.period_close_form.reason);
        let preview = (|| -> Result<PeriodClose> {
            let terms = PeriodCloseTerms {
                id: self.period_close_form.id.clone(),
                start: Date::parse(&self.period_close_form.start)?,
                end: Date::parse(&self.period_close_form.end)?,
                currency: self.period_close_form.currency,
                reason: self.period_close_form.reason.clone(),
            };
            self.engine
                .state()
                .period_close_preview(&terms, &self.actor().name)
        })();
        match &preview {
            Ok(c) => Self::period_close_basis(ui, c),
            Err(e) => {
                ui.colored_label(COPPER, format!("Period close unavailable: {e}"));
            }
        }
        if ui
            .add_enabled(
                preview.is_ok() && matches!(self.actor().role, Role::Analyst | Role::Admin),
                egui::Button::new("Create period close proposal"),
            )
            .clicked()
            && let Ok(c) = preview
        {
            let id = c.terms.id.clone();
            self.run(Command::PeriodCloseCreate { terms: c.terms });
            if !self.failed {
                self.period_close_form.selected = id;
                self.period_close_form.prepare_open = Some(false);
            }
        }
        ui.small("Empty intervals may close. Start is inclusive and end is exclusive. Adjacent periods and other recorded currencies remain independent.");
    }

    fn period_close_basis(ui: &mut egui::Ui, c: &PeriodClose) {
        ui.strong(format!(
            "{} {} net invoice / {} cost · source r{}",
            c.terms.currency, c.basis.invoice, c.basis.cost, c.source_revision
        ));
        ui.label(format!(
            "{} original invoice records · {} full reversals · {} quantity credit notes",
            c.basis.transaction_ids.len(),
            c.basis.correction_ids.len(),
            c.basis.credit_ids.len()
        ));
        ui.push_id(("close-basis", &c.terms.id, c.source_revision), |ui| {
            egui::Grid::new("nominal-close-basis")
                .striped(true)
                .spacing([24.0, 10.0])
                .show(ui, |ui| {
                    for h in ["Nominal basis", "Invoice", "Cost"] {
                        ui.strong(h);
                    }
                    ui.end_row();
                    for (label, invoice, cost) in [
                        ("Gross originals", c.basis.gross_invoice, c.basis.gross_cost),
                        (
                            "Full reversals",
                            c.basis.reversed_invoice,
                            c.basis.reversed_cost,
                        ),
                        (
                            "Quantity credits",
                            c.basis.credited_invoice,
                            c.basis.credited_cost,
                        ),
                        ("Net basis", c.basis.invoice, c.basis.cost),
                    ] {
                        ui.label(label);
                        ui.label(invoice.to_string());
                        ui.label(cost.to_string());
                        ui.end_row();
                    }
                });
        });
    }
}
