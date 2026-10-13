use super::*;
#[derive(Default)]
pub(super) struct SettlementForm {
    accrual: String,
    selected: String,
    reason: String,
    export_path: String,
}
impl Workbench {
    pub(super) fn settlements_view(&mut self, ui: &mut egui::Ui) {
        ui.label("Reconcile corrected full-period rebate entitlement against approved local settlement adjustments. These records do not execute payments, create ERP postings or prove cash settlement.");
        let agreements: Vec<_> = self
            .engine
            .state()
            .agreements
            .values()
            .filter(|a| a.status == Status::Approved)
            .map(|a| a.terms.id.clone())
            .collect();
        if !agreements.contains(&self.selected_agreement) {
            self.selected_agreement = agreements.first().cloned().unwrap_or_default();
        }
        egui::ComboBox::from_id_salt("settlement-agreement")
            .selected_text(&self.selected_agreement)
            .show_ui(ui, |ui| {
                for id in agreements {
                    ui.selectable_value(&mut self.selected_agreement, id.clone(), id);
                }
            });
        match self
            .engine
            .state()
            .rebate_position(&self.selected_agreement, self.engine.state().revision)
        {
            Ok(position) => {
                ui.heading(format!(
                    "{} {} remaining adjustment",
                    position.currency, position.remaining_adjustment
                ));
                egui::Grid::new("reconciliation-position")
                    .spacing([28.0, 10.0])
                    .show(ui, |ui| {
                        for (label, value) in [
                            ("Gross eligible invoices", position.basis.gross_invoice),
                            ("Approved full reversals", position.basis.reversed_invoice),
                            ("Net eligible invoices", position.basis.eligible_invoice),
                            ("Full-period rebate target", position.target_rebate),
                            ("Approved local adjustments", position.recognized_rebate),
                        ] {
                            ui.label(label);
                            ui.label(format!("{} {}", position.currency, value));
                            ui.end_row();
                        }
                    });
                ui.label(format!("Retrospective tier {}% · {} invoice records · {} approved reversals · source r{}",position.rate,position.basis.transaction_ids.len(),position.basis.correction_ids.len(),position.source_revision));
                if primary(ui, "Calculate corrected accrual")
                    && let Some(a) = self.engine.state().agreements.get(&self.selected_agreement)
                {
                    let id = format!("R-{:04}", self.engine.state().revision + 1);
                    self.run(Command::RebateAccrue {
                        id: id.clone(),
                        agreement_id: a.terms.id.clone(),
                        version: a.version,
                    });
                    if !self.failed {
                        self.settlement_form.accrual = id;
                    }
                }
            }
            Err(e) => {
                ui.colored_label(COPPER, e.to_string());
            }
        }
        ui.separator();
        let accruals: Vec<_> = self
            .engine
            .state()
            .accruals
            .values()
            .filter(|a| a.agreement_id == self.selected_agreement)
            .cloned()
            .collect();
        if !accruals
            .iter()
            .any(|a| a.id == self.settlement_form.accrual)
        {
            self.settlement_form.accrual = accruals
                .iter()
                .max_by_key(|a| a.source_revision)
                .map(|a| a.id.clone())
                .unwrap_or_default();
        }
        egui::ComboBox::from_id_salt("settlement-accrual")
            .selected_text(&self.settlement_form.accrual)
            .show_ui(ui, |ui| {
                for a in accruals {
                    ui.selectable_value(
                        &mut self.settlement_form.accrual,
                        a.id.clone(),
                        format!(
                            "{} · r{} · {} {}",
                            a.id, a.source_revision, a.currency, a.rebate
                        ),
                    );
                }
            });
        if self.settlement_form.reason.is_empty() {
            self.settlement_form.reason = "Reconcile synthetic agreement entitlement".into();
        }
        field(
            ui,
            "Reconciliation reason",
            &mut self.settlement_form.reason,
        );
        if primary(ui, "Create settlement proposal") {
            let id = format!("ST-{:04}", self.engine.state().revision + 1);
            self.run(Command::SettlementCreate {
                id: id.clone(),
                accrual_id: self.settlement_form.accrual.clone(),
                reason: self.settlement_form.reason.clone(),
            });
            if !self.failed {
                self.settlement_form.selected = id;
            }
        }
        ui.small("Select a current accrual. Matching totals alone are insufficient: changed invoice or correction provenance requires a new snapshot.");
        ui.separator();
        ui.heading("Settlement records");
        let records: Vec<_> = self
            .engine
            .state()
            .settlements
            .values()
            .filter(|s| s.agreement_id == self.selected_agreement)
            .cloned()
            .collect();
        if !records
            .iter()
            .any(|s| s.id == self.settlement_form.selected)
        {
            self.settlement_form.selected =
                records.last().map(|s| s.id.clone()).unwrap_or_default();
        }
        egui::ComboBox::from_id_salt("settlement-selected")
            .selected_text(&self.settlement_form.selected)
            .show_ui(ui, |ui| {
                for s in &records {
                    ui.selectable_value(
                        &mut self.settlement_form.selected,
                        s.id.clone(),
                        format!(
                            "{} · {:?} · {} {}",
                            s.id, s.status, s.currency, s.adjustment
                        ),
                    );
                }
            });
        if let Some(record) = records
            .iter()
            .find(|s| s.id == self.settlement_form.selected)
        {
            ui.label(format!(
                "{} · {:?} · v{} · owner {}",
                record.id, record.status, record.version, record.owner
            ));
            ui.label(format!(
                "{} {} target − {} recognized = {} adjustment",
                record.currency,
                record.target_rebate,
                record.previously_recognized,
                record.adjustment
            ));
            ui.label(format!(
                "Accrual {} · previous approved records: {}",
                record.accrual_id,
                if record.prior_settlement_ids.is_empty() {
                    "None".into()
                } else {
                    record.prior_settlement_ids.join(", ")
                }
            ));
            ui.small(format!(
                "Excluded contributors: {}",
                record
                    .contributors
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            if let Some(review) = &record.review {
                ui.label(format!(
                    "{} by {} at r{} · {}",
                    if review.approved {
                        "Approved"
                    } else {
                        "Rejected"
                    },
                    review.actor.name,
                    review.revision,
                    review.note
                ));
            }
            if matches!(record.status, RecordStatus::Draft | RecordStatus::Pending)
                && let Err(e) = self
                    .engine
                    .state()
                    .ensure_settlement_current(record, self.engine.state().revision)
            {
                ui.colored_label(COPPER, e.to_string());
            }
            field(ui, "Decision note", &mut self.note);
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(
                        record.status == RecordStatus::Draft,
                        egui::Button::new("Submit settlement"),
                    )
                    .clicked()
                {
                    self.run(Command::SettlementSubmit {
                        id: record.id.clone(),
                        version: record.version,
                    });
                }
                if ui
                    .add_enabled(
                        record.status == RecordStatus::Pending,
                        egui::Button::new("Approve settlement"),
                    )
                    .clicked()
                {
                    self.run(Command::SettlementDecide {
                        id: record.id.clone(),
                        version: record.version,
                        approve: true,
                        note: self.note.clone(),
                    });
                }
                if ui
                    .add_enabled(
                        record.status == RecordStatus::Pending,
                        egui::Button::new("Reject settlement"),
                    )
                    .clicked()
                {
                    self.run(Command::SettlementDecide {
                        id: record.id.clone(),
                        version: record.version,
                        approve: false,
                        note: self.note.clone(),
                    });
                }
                if ui
                    .add_enabled(
                        matches!(record.status, RecordStatus::Draft | RecordStatus::Pending),
                        egui::Button::new("Withdraw settlement"),
                    )
                    .clicked()
                {
                    self.run(Command::SettlementWithdraw {
                        id: record.id.clone(),
                        version: record.version,
                    });
                }
            });
            if self.settlement_form.export_path.is_empty() {
                self.settlement_form.export_path = "approved-settlement.csv".into();
            }
            field(ui, "Export file", &mut self.settlement_form.export_path);
            if ui
                .add_enabled(
                    record.status == RecordStatus::Approved,
                    egui::Button::new("Export settlement evidence"),
                )
                .clicked()
            {
                let result = (|| -> Result<()> {
                    use std::io::Write;
                    let csv =
                        copperline_engine::export_settlement(self.engine.state(), &record.id)?;
                    let mut file = std::fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&self.settlement_form.export_path)
                        .map_err(|e| error(format!("Choose a new export path: {e}")))?;
                    file.write_all(csv.as_bytes())
                        .map_err(|e| error(e.to_string()))?;
                    Ok(())
                })();
                self.result(
                    result,
                    "Frozen settlement evidence exported; no payment or posting.",
                );
            }
        }
    }
}
