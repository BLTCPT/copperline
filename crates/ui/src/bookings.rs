use super::*;

#[cfg(test)]
mod tests;

pub(super) struct BookingForm {
    quote_id: String,
    loaded_quote: String,
    id: String,
    invoice_date: String,
    source: String,
    selected: String,
    export_path: String,
    prepare_open: Option<bool>,
}
impl Default for BookingForm {
    fn default() -> Self {
        Self {
            quote_id: String::new(),
            loaded_quote: String::new(),
            id: String::new(),
            invoice_date: String::new(),
            source: "synthetic-quote-booking".into(),
            selected: String::new(),
            export_path: "quote-booking.csv".into(),
            prepare_open: None,
        }
    }
}
impl Workbench {
    pub(super) fn booking_quote_shortcut(&mut self, ui: &mut egui::Ui, quote: &Quote) {
        let existing = self
            .engine
            .state()
            .quote_bookings
            .values()
            .find(|b| b.request.quote_id == quote.id)
            .map(|b| b.request.id.clone());
        if let Some(id) = existing {
            if ui.button(format!("View invoice booking · {id}")).clicked() {
                self.booking_form.selected = id;
                self.booking_form.prepare_open = Some(false);
                self.view = View::Bookings;
            }
        } else if quote.status == Status::Approved
            && quote.governance.is_some()
            && ui.button("Book approved quote").clicked()
        {
            self.booking_form.quote_id = quote.id.clone();
            self.booking_form.loaded_quote.clear();
            self.booking_form.prepare_open = Some(true);
            self.view = View::Bookings;
        }
    }

    pub(super) fn bookings_view(&mut self, ui: &mut egui::Ui) {
        ui.label("Book every line of an approved quote as immutable recorded invoice actuals. Exact approved amounts and conversion evidence are retained; no ERP posting or payment is performed.");
        ui.colored_label(COPPER, "One full booking per quote, permanently. Reversing an invoice does not release the quote for another booking. Partial quantities are not supported.");
        let eligible: Vec<_> = self
            .engine
            .state()
            .quotes
            .values()
            .filter(|q| {
                q.status == Status::Approved
                    && q.governance.is_some()
                    && !self
                        .engine
                        .state()
                        .quote_bookings
                        .values()
                        .any(|b| b.request.quote_id == q.id)
            })
            .cloned()
            .collect();
        if !eligible.iter().any(|q| q.id == self.booking_form.quote_id) {
            self.booking_form.quote_id = eligible.first().map(|q| q.id.clone()).unwrap_or_default();
        }
        ui.add_space(12.0);
        egui::CollapsingHeader::new("Prepare a full quote booking")
            .id_salt("booking-prepare")
            .default_open(self.engine.state().quote_bookings.is_empty())
            .open(self.booking_form.prepare_open.take())
            .show(ui, |ui| self.prepare_quote_booking(ui, &eligible));
        ui.separator();
        ui.heading("Immutable invoice bookings");
        let records: Vec<_> = self
            .engine
            .state()
            .quote_bookings
            .values()
            .cloned()
            .collect();
        if !records
            .iter()
            .any(|b| b.request.id == self.booking_form.selected)
        {
            self.booking_form.selected = records
                .iter()
                .max_by_key(|b| b.created_revision)
                .map(|b| b.request.id.clone())
                .unwrap_or_default();
        }
        if records.is_empty() {
            ui.label("No booked quotes yet.");
            return;
        }
        egui::ComboBox::from_id_salt("booking-record")
            .selected_text(&self.booking_form.selected)
            .show_ui(ui, |ui| {
                for b in &records {
                    ui.selectable_value(
                        &mut self.booking_form.selected,
                        b.request.id.clone(),
                        format!(
                            "{} · {} v{} · r{}",
                            b.request.id,
                            b.request.quote_id,
                            b.request.quote_version,
                            b.created_revision
                        ),
                    );
                }
            });
        if let Some(b) = records
            .iter()
            .find(|b| b.request.id == self.booking_form.selected)
        {
            ui.label(format!(
                "{} · quote {} v{} · booked r{} by {} · invoice date {}",
                b.request.id,
                b.request.quote_id,
                b.request.quote_version,
                b.created_revision,
                b.owner,
                b.request.invoice_date
            ));
            ui.label(format!("Source: {}", b.request.source));
            Self::booking_totals(ui, &b.approval_cycle.lines);
            Self::booking_lines(ui, "booking-record-lines", &b.lines);
            egui::CollapsingHeader::new("Frozen approval and pricing evidence").show(ui, |ui| {
                ui.label(format!(
                    "Submitted by {} at r{} · pricing data r{} · contributors: {}",
                    b.approval_cycle.submitted_by,
                    b.approval_cycle.submitted_revision,
                    b.approval_cycle.source_revision,
                    b.quote_contributors
                        .iter()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
                for review in &b.approval_cycle.reviews {
                    ui.label(format!(
                        "Approved by {} ({:?}) at r{} · {}",
                        review.actor.name, review.actor.role, review.revision, review.note
                    ));
                }
                Self::line_conversion_evidence(ui, &b.approval_cycle.lines);
                match serde_json::to_string_pretty(b) {
                    Ok(json) => {
                        ui.monospace(json);
                    }
                    Err(e) => {
                        ui.colored_label(Color32::DARK_RED, e.to_string());
                    }
                }
            });
            ui.horizontal_wrapped(|ui| {
                for line in &b.lines {
                    let reversal = self.engine.state().corrections.values().find(|c| {
                        c.proposal.original_transaction_id == line.booked.id
                            && c.status == RecordStatus::Approved
                    });
                    if let Some(c) = reversal {
                        ui.label(format!(
                            "{} fully reversed by {}",
                            line.booked.id, c.proposal.id
                        ));
                    } else if ui
                        .button(format!("Full reversal · {}", line.booked.id))
                        .clicked()
                    {
                        self.open_booked_reversal(&line.booked.id);
                    }
                }
            });
            ui.small("Correction review reverses a whole booked line. Any replacement requires a distinct approved quote and a new booking.");
            field(ui, "Booking CSV file", &mut self.booking_form.export_path);
            if ui.button("Export booking CSV").clicked() {
                let result = conversions::write_export(
                    &self.booking_form.export_path,
                    copperline_engine::export_booking(self.engine.state(), &b.request.id),
                );
                self.result(
                    result,
                    "Immutable booking CSV exported. Existing files are never overwritten.",
                );
            }
            egui::CollapsingHeader::new("Preview booking CSV").show(ui, |ui| {
                match copperline_engine::export_booking(self.engine.state(), &b.request.id) {
                    Ok(csv) => {
                        ui.monospace(csv);
                    }
                    Err(e) => {
                        ui.colored_label(Color32::DARK_RED, e.to_string());
                    }
                }
            });
        }
    }

    fn prepare_quote_booking(&mut self, ui: &mut egui::Ui, eligible: &[Quote]) {
        if eligible.is_empty() {
            ui.label("No eligible quotes. Create and independently approve a new quote, or revise and reapprove a legacy quote. Already booked quote IDs cannot be booked again.");
        } else {
            egui::ComboBox::from_id_salt("booking-quote")
                .selected_text(&self.booking_form.quote_id)
                .show_ui(ui, |ui| {
                    for q in eligible {
                        ui.selectable_value(
                            &mut self.booking_form.quote_id,
                            q.id.clone(),
                            format!("{} · v{} · {}", q.id, q.version, q.title),
                        );
                    }
                });
            if let Some(quote) = eligible.iter().find(|q| q.id == self.booking_form.quote_id) {
                if self.booking_form.loaded_quote != quote.id {
                    self.booking_form.loaded_quote = quote.id.clone();
                    self.booking_form.invoice_date = quote.date.to_string();
                    if self.booking_form.id.is_empty() {
                        self.booking_form.id =
                            format!("BK-{:04}", self.engine.state().revision.saturating_add(1));
                    }
                }
                ui.label(format!(
                    "{} · current approved v{} · {} · pricing date {} · {} lines",
                    quote.id,
                    quote.version,
                    quote.customer_id,
                    quote.date,
                    quote.lines.len()
                ));
                field(ui, "New booking ID", &mut self.booking_form.id);
                field(ui, "Invoice date", &mut self.booking_form.invoice_date);
                field(ui, "Source label", &mut self.booking_form.source);
                ui.small("Invoice date must be on or after the approved pricing date. Booking IDs may contain at most 59 characters; invoice IDs append -L001, -L002, and so on.");
                let preview = (|| -> Result<(BookingRequest, Vec<BookedQuoteLine>)> {
                    let request = BookingRequest {
                        id: self.booking_form.id.clone(),
                        quote_id: quote.id.clone(),
                        quote_version: quote.version,
                        invoice_date: Date::parse(&self.booking_form.invoice_date)?,
                        source: self.booking_form.source.clone(),
                    };
                    let lines = self.engine.state().preview_quote_booking(&request)?;
                    Ok((request, lines))
                })();
                ui.add_space(10.0);
                match &preview {
                    Ok((_, lines)) => {
                        ui.strong("Exact approved booking preview");
                        Self::booking_totals(ui, &quote.lines);
                        Self::booking_lines(ui, "booking-preview", lines);
                    }
                    Err(e) => {
                        ui.colored_label(Color32::DARK_RED, format!("Booking unavailable: {e}"));
                    }
                }
                let can_book = matches!(self.actor().role, Role::Analyst | Role::Admin);
                if !can_book {
                    ui.small("Select the Pricing or Admin demo identity to book. Other roles can inspect this preview.");
                }
                if ui
                    .add_enabled(
                        preview.is_ok() && can_book,
                        egui::Button::new(
                            RichText::new("Book entire approved quote").color(Color32::WHITE),
                        )
                        .fill(COPPER)
                        .min_size(egui::vec2(220.0, 34.0)),
                    )
                    .clicked()
                    && let Ok((request, _)) = preview
                {
                    let id = request.id.clone();
                    self.run(Command::QuoteBook { request });
                    if !self.failed {
                        self.booking_form.selected = id;
                        self.booking_form.id.clear();
                        self.booking_form.loaded_quote.clear();
                        self.booking_form.prepare_open = Some(false);
                    }
                }
                Self::line_conversion_evidence(ui, &quote.lines);
            }
        }
    }

    fn booking_totals(ui: &mut egui::Ui, lines: &[Evaluation]) {
        match totals(lines) {
            Ok(t) => {
                ui.strong(format!(
                    "Recorded invoice {} {} · recorded cost {} {}",
                    t.currency, t.invoice, t.currency, t.cost
                ));
            }
            Err(e) => {
                ui.colored_label(Color32::DARK_RED, e.to_string());
            }
        }
        ui.small("Approved invoice and cost totals are authoritative. Projected quote rebates are not booked or settled by this action.");
    }

    fn booking_lines(ui: &mut egui::Ui, salt: &str, lines: &[BookedQuoteLine]) {
        egui::Grid::new(salt)
            .striped(true)
            .spacing([18.0, 10.0])
            .show(ui, |ui| {
                for label in [
                    "Invoice / product",
                    "Recorded quantity",
                    "Recorded invoice / cost",
                    "Nonposted native basis",
                    "Rounding difference",
                ] {
                    ui.strong(label);
                }
                ui.end_row();
                for line in lines {
                    let t = &line.booked;
                    let n = &line.native_basis;
                    ui.label(format!("{}\n{}", t.id, t.product_id));
                    ui.label(format!("{} {}", t.quantity, t.unit));
                    ui.label(format!(
                        "{} {} / {}",
                        t.currency, t.invoice_total, t.cost_total
                    ));
                    ui.label(format!(
                        "{} {}\n{} {} / {}",
                        n.quantity, n.unit, n.currency, n.invoice_total, n.cost_total
                    ));
                    ui.label(format!(
                        "{} {} / {}",
                        t.currency, line.reconciliation.invoice, line.reconciliation.cost
                    ));
                    ui.end_row();
                }
            });
        ui.small("Differences = recorded totals minus the nonposted native totals converted forward using the quote's frozen FX. Unit and extension rounding can differ; no reverse conversion or live repricing is used.");
    }
}
