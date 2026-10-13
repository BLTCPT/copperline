use super::*;
#[derive(Default)]
pub(super) struct CorrectionForm {
    invoice: String,
    loaded: String,
    selected: String,
    reason: String,
    record_date: String,
    replace: bool,
    replacement_id: String,
    replacement_date: String,
    quantity: String,
    invoice_total: String,
    cost_total: String,
}
impl Workbench {
    pub(super) fn open_booked_reversal(&mut self, invoice: &str) {
        self.correction_form.invoice = invoice.into();
        self.correction_form.loaded.clear();
        self.correction_form.replace = false;
        self.view = View::Corrections;
    }

    pub(super) fn corrections_view(&mut self, ui: &mut egui::Ui) {
        ui.label("Correct recorded actuals through a full reversal, optionally with an atomic replacement. Originals and earlier accruals remain immutable. No payment or ERP posting.");
        let ids: Vec<_> = self.engine.state().transactions.keys().cloned().collect();
        if self.correction_form.invoice.is_empty() {
            self.correction_form.invoice = self
                .engine
                .state()
                .corrections
                .values()
                .max_by_key(|c| c.created_revision)
                .map(|c| c.proposal.original_transaction_id.clone())
                .or_else(|| ids.first().cloned())
                .unwrap_or_default();
        }
        egui::ComboBox::from_id_salt("correction-invoice")
            .selected_text(&self.correction_form.invoice)
            .show_ui(ui, |ui| {
                for id in ids {
                    ui.selectable_value(&mut self.correction_form.invoice, id.clone(), id);
                }
            });
        if let Some(original) = self
            .engine
            .state()
            .transactions
            .get(&self.correction_form.invoice)
            .cloned()
        {
            if self.correction_form.loaded != original.id {
                self.correction_form.loaded = original.id.clone();
                self.correction_form.replacement_id = format!("{}-R", original.id);
                self.correction_form.replacement_date = original.date.to_string();
                self.correction_form.record_date = original.date.to_string();
                self.correction_form.quantity = original.quantity.to_string();
                self.correction_form.invoice_total = original.invoice_total.to_string();
                self.correction_form.cost_total = original.cost_total.to_string();
                self.correction_form.reason = "Correct synthetic invoice data".into();
            }
            ui.label(format!(
                "Original {} · {} · {} · {} {} · {} {}",
                original.id,
                original.customer_id,
                original.product_id,
                original.currency,
                original.invoice_total,
                original.quantity,
                original.unit
            ));
            field(
                ui,
                "Correction record date",
                &mut self.correction_form.record_date,
            );
            field(ui, "Reason", &mut self.correction_form.reason);
            ui.small("The record date is provenance, not scheduled activation. Approval immediately restates the original invoice period.");
            let booking = self
                .engine
                .state()
                .booking_for_transaction(&original.id)
                .map(|(b, _)| (b.request.id.clone(), b.request.quote_id.clone()));
            if let Some((booking, quote)) = &booking {
                self.correction_form.replace = false;
                ui.colored_label(COPPER, format!("Booked invoice · {booking} / {quote}. Only a full reversal is allowed. A replacement requires a distinct independently approved quote and a new booking."));
            }
            ui.add_enabled_ui(booking.is_none(), |ui| {
                ui.checkbox(
                    &mut self.correction_form.replace,
                    "Replace with a corrected invoice atomically",
                );
            });
            if self.correction_form.replace {
                field(
                    ui,
                    "New external ID",
                    &mut self.correction_form.replacement_id,
                );
                field(
                    ui,
                    "Replacement date",
                    &mut self.correction_form.replacement_date,
                );
                field(ui, "Quantity", &mut self.correction_form.quantity);
                field(
                    ui,
                    "Invoice amount",
                    &mut self.correction_form.invoice_total,
                );
                field(ui, "Cost amount", &mut self.correction_form.cost_total);
                ui.small("This form preserves customer, product, currency and unit. The command API also supports explicit context corrections without FX conversion.");
            }
            if primary(ui, "Create correction proposal") {
                let proposal = (|| -> Result<CorrectionProposal> {
                    let replacement = if self.correction_form.replace {
                        let mut t = original.clone();
                        t.id = self.correction_form.replacement_id.clone();
                        t.source = "synthetic-correction".into();
                        t.date = Date::parse(&self.correction_form.replacement_date)?;
                        t.quantity = self.correction_form.quantity.parse()?;
                        t.invoice_total = self.correction_form.invoice_total.parse()?;
                        t.cost_total = self.correction_form.cost_total.parse()?;
                        Some(t)
                    } else {
                        None
                    };
                    Ok(CorrectionProposal {
                        id: format!("CR-{:04}", self.engine.state().revision + 1),
                        original_transaction_id: original.id.clone(),
                        effective_date: Date::parse(&self.correction_form.record_date)?,
                        reason: self.correction_form.reason.clone(),
                        replacement,
                    })
                })();
                match proposal {
                    Ok(proposal) => {
                        let id = proposal.id.clone();
                        self.run(Command::CorrectionCreate { proposal });
                        if !self.failed {
                            self.correction_form.selected = id;
                        }
                    }
                    Err(e) => self.result::<()>(Err(e), ""),
                }
            }
        } else {
            ui.label("Record an invoice in Invoice analytics first.");
        }
        ui.separator();
        ui.heading("Correction review and provenance");
        let records: Vec<_> = self.engine.state().corrections.values().cloned().collect();
        if self.correction_form.selected.is_empty() {
            self.correction_form.selected = records
                .last()
                .map(|r| r.proposal.id.clone())
                .unwrap_or_default();
        }
        egui::ComboBox::from_id_salt("correction-selected")
            .selected_text(&self.correction_form.selected)
            .show_ui(ui, |ui| {
                for r in &records {
                    ui.selectable_value(
                        &mut self.correction_form.selected,
                        r.proposal.id.clone(),
                        format!("{} · {:?}", r.proposal.id, r.status),
                    );
                }
            });
        if let Some(record) = records
            .iter()
            .find(|r| r.proposal.id == self.correction_form.selected)
        {
            ui.label(format!(
                "{} · {:?} · v{} · owner {} · created r{}",
                record.proposal.id,
                record.status,
                record.version,
                record.owner,
                record.created_revision
            ));
            ui.label(format!(
                "Reversal: {} {} invoice / {} cost / {} {} · original {}",
                record.original.currency,
                record.original.invoice_total,
                record.original.cost_total,
                record.original.quantity,
                record.original.unit,
                record.original.id
            ));
            if let Some(replacement) = &record.proposal.replacement {
                ui.label(format!(
                    "Replacement: {} · {} {} invoice / {} cost · {}",
                    replacement.id,
                    replacement.currency,
                    replacement.invoice_total,
                    replacement.cost_total,
                    replacement.date
                ));
            }
            ui.label(&record.proposal.reason);
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
            field(ui, "Decision note", &mut self.note);
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(
                        record.status == RecordStatus::Draft,
                        egui::Button::new("Submit correction"),
                    )
                    .clicked()
                {
                    self.run(Command::CorrectionSubmit {
                        id: record.proposal.id.clone(),
                        version: record.version,
                    });
                }
                if ui
                    .add_enabled(
                        record.status == RecordStatus::Pending,
                        egui::Button::new("Approve correction"),
                    )
                    .clicked()
                {
                    self.run(Command::CorrectionDecide {
                        id: record.proposal.id.clone(),
                        version: record.version,
                        approve: true,
                        note: self.note.clone(),
                    });
                }
                if ui
                    .add_enabled(
                        record.status == RecordStatus::Pending,
                        egui::Button::new("Reject correction"),
                    )
                    .clicked()
                {
                    self.run(Command::CorrectionDecide {
                        id: record.proposal.id.clone(),
                        version: record.version,
                        approve: false,
                        note: self.note.clone(),
                    });
                }
                if ui
                    .add_enabled(
                        matches!(record.status, RecordStatus::Draft | RecordStatus::Pending),
                        egui::Button::new("Withdraw correction"),
                    )
                    .clicked()
                {
                    self.run(Command::CorrectionWithdraw {
                        id: record.proposal.id.clone(),
                        version: record.version,
                    });
                }
            });
        }
        ui.small("The engine enforces ownership, reserved replacement IDs, one full reversal per invoice and independent Finance review. Partial credits are not supported.");
    }
}
