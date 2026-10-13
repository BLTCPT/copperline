use super::*;
use copperline_engine::QuoteLineReplacement;
pub(super) struct ConversionForm {
    pub enabled: bool,
    pub currency: Currency,
    pub unit: String,
    id: String,
    name: String,
    csv: String,
    selected: String,
    start: String,
    end: String,
    report_id: String,
    report_currency: Currency,
    selected_report: String,
    batch_quote: String,
    batch_version: u64,
    batch: Vec<QuoteLineReplacement>,
}
impl Default for ConversionForm {
    fn default() -> Self {
        Self {enabled:false,currency:Currency::EUR,unit:"box".into(),id:"FX-UOM-OCT".into(),name:"Synthetic October conversion schedule".into(),
        csv:"id,kind,product_id,from,to,start,end,numerator,denominator\nUSD-EUR,fx,,USD,EUR,2026-10-01,2026-11-01,9,10\nVLV-BOX,uom,VLV-100,box,each,2026-10-01,2026-11-01,12,1\n".into(),selected:String::new(),start:"2026-10-01".into(),end:"2026-11-01".into(),report_id:"R-EUR-OCT".into(),report_currency:Currency::EUR,selected_report:String::new(),batch_quote:String::new(),batch_version:0,batch:vec![]}
    }
}
fn currency_picker(ui: &mut egui::Ui, key: &str, currency: &mut Currency) {
    egui::ComboBox::from_id_salt(key)
        .selected_text(currency.to_string())
        .show_ui(ui, |ui| {
            for c in [Currency::USD, Currency::EUR, Currency::GBP] {
                ui.selectable_value(currency, c, c.to_string());
            }
        });
}
pub(super) fn write_export(path: &str, content: Result<String>) -> Result<()> {
    use std::io::Write;
    let content = content?;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| error(e.to_string()))?;
    f.write_all(content.as_bytes())
        .map_err(|e| error(e.to_string()))
}
impl Workbench {
    pub fn select_conversion(&mut self, currency: &str, unit: &str) -> Result<()> {
        let currency = match currency {
            "USD" => Currency::USD,
            "EUR" => Currency::EUR,
            "GBP" => Currency::GBP,
            _ => return Err(error("Choose USD, EUR or GBP")),
        };
        let mut request = self.request()?;
        request.conversion = Some(ConversionRequest {
            currency,
            unit: unit.into(),
        });
        evaluate(self.engine.state(), &request)?;
        self.conversion_form.enabled = true;
        self.conversion_form.currency = currency;
        self.conversion_form.unit = unit.into();
        Ok(())
    }
    pub(super) fn conversion_picker(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(
                &mut self.conversion_form.enabled,
                "Convert quote currency / unit",
            );
            if self.conversion_form.enabled {
                currency_picker(
                    ui,
                    "quote-output-currency",
                    &mut self.conversion_form.currency,
                );
                ui.label("Requested unit");
                ui.add(
                    egui::TextEdit::singleline(&mut self.conversion_form.unit).desired_width(100.0),
                );
                ui.small("Quantity is in this unit. Approved direct schedules only.");
            }
        });
        if let Some(p) = self.engine.state().products.get(&self.product) {
            ui.small(format!(
                "Native pricing and quantity bands: {} / {}. Catalog rows remain native.",
                p.currency, p.unit
            ));
        }
    }
    pub(super) fn line_conversion_evidence(ui: &mut egui::Ui, lines: &[Evaluation]) {
        for (i, line) in lines
            .iter()
            .enumerate()
            .filter(|(_, l)| l.conversion.is_some())
        {
            egui::CollapsingHeader::new(format!(
                "Line {} · frozen conversion and price evidence",
                i + 1
            ))
            .show(ui, |ui| {
                ui.push_id(("line-conversion", i), |ui| Self::waterfall(ui, line));
            });
        }
    }
    pub(super) fn conversion_evidence(ui: &mut egui::Ui, v: &Evaluation) {
        if let Some(c) = &v.conversion {
            ui.colored_label(
                TEAL,
                format!(
                    "{} {} = {} {} · {} to {}",
                    v.request.quantity,
                    v.unit,
                    c.native_quantity,
                    c.native_unit,
                    c.native_currency,
                    v.currency
                ),
            );
            for r in [&c.fx, &c.uom].into_iter().flatten() {
                ui.small(format!(
                    "{} × {}/{} · {} / {} v{} · [{} , {}) · approved r{} / captured r{}",
                    r.entry.conversion.label(),
                    r.entry.ratio.numerator,
                    r.entry.ratio.denominator,
                    r.book_id,
                    r.entry.id,
                    r.book_version,
                    r.entry.start,
                    r.entry.end,
                    r.approval_revision,
                    r.captured_revision
                ));
            }
            ui.small("Native unit × UOM × FX, rounded once to cents; discount and quantity follow. Unit changes can change rounded totals.");
        }
    }
    pub(super) fn conversions_view(&mut self, ui: &mut egui::Ui) {
        ui.label("Explicit, dated conversion schedules. Native units per requested unit; target currency per source currency. Each book needs independent commercial approval.");
        ui.small("No inverse or intermediate conversion is inferred. Approved rows are immutable; create a new book for an adjacent period. Overlapping amendments are not supported.");
        egui::CollapsingHeader::new("Prepare a conversion book").show(ui,|ui|{
            field(ui,"Book ID",&mut self.conversion_form.id);field(ui,"Name",&mut self.conversion_form.name);
            ui.label("Paste CSV: fx rows leave product_id empty; uom rows name a product and map directly to its native unit. Ratio = numerator / denominator.");
            ui.add(egui::TextEdit::multiline(&mut self.conversion_form.csv).font(egui::TextStyle::Monospace).desired_width(f32::INFINITY).desired_rows(5));
            if primary(ui,"Import draft conversion book") {
                let id=self.conversion_form.id.clone();self.run(Command::ConversionImport {id:id.clone(),name:self.conversion_form.name.clone(),csv:self.conversion_form.csv.clone()});
                if !self.failed {self.conversion_form.selected=id;}
            }
        });
        ui.add_space(14.0);
        let books: Vec<_> = self
            .engine
            .state()
            .conversion_books
            .values()
            .cloned()
            .collect();
        if self.conversion_form.selected.is_empty() {
            self.conversion_form.selected =
                books.last().map(|b| b.terms.id.clone()).unwrap_or_default();
        }
        egui::Grid::new("conversion-books")
            .striped(true)
            .spacing([24.0, 12.0])
            .show(ui, |ui| {
                for h in ["Book", "Name", "Rows", "Version", "Status"] {
                    ui.strong(h);
                }
                ui.end_row();
                for b in &books {
                    ui.selectable_value(
                        &mut self.conversion_form.selected,
                        b.terms.id.clone(),
                        &b.terms.id,
                    );
                    ui.label(&b.terms.name);
                    ui.label(b.terms.entries.len().to_string());
                    ui.label(b.version.to_string());
                    ui.label(format!("{:?}", b.status));
                    ui.end_row();
                }
            });
        let Some(b) = self
            .engine
            .state()
            .conversion_books
            .get(&self.conversion_form.selected)
            .cloned()
        else {
            return;
        };
        ui.add_space(18.0);
        ui.heading(&b.terms.name);
        ui.label(format!(
            "Owner {} · created r{}",
            b.owner, b.created_revision
        ));
        egui::Grid::new("conversion-rows")
            .striped(true)
            .spacing([22.0, 12.0])
            .show(ui, |ui| {
                for h in [
                    "Row",
                    "Explicit direction",
                    "Exact multiplier",
                    "Valid from",
                    "Until (exclusive)",
                ] {
                    ui.strong(h);
                }
                ui.end_row();
                for r in &b.terms.entries {
                    ui.label(&r.id);
                    ui.label(r.conversion.label());
                    ui.label(format!("{} / {}", r.ratio.numerator, r.ratio.denominator));
                    ui.label(r.start.to_string());
                    ui.label(r.end.to_string());
                    ui.end_row();
                }
            });
        if let Some(r) = &b.review {
            ui.label(format!(
                "{} by {} at r{} · {}",
                if r.approved { "Approved" } else { "Rejected" },
                r.actor.name,
                r.revision,
                r.note
            ));
        }
        field(ui, "Review note", &mut self.note);
        let a = self.actor();
        let owner = a.name == b.owner && matches!(a.role, Role::Analyst | Role::Admin);
        let reviewer = a.name != b.owner && a.role == Role::Approver;
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    owner && b.status == RecordStatus::Draft,
                    egui::Button::new("Submit conversion book"),
                )
                .clicked()
            {
                self.run(Command::ConversionSubmit {
                    id: b.terms.id.clone(),
                    version: b.version,
                });
            }
            for (label, approve) in [
                ("Approve conversion book", true),
                ("Reject conversion book", false),
            ] {
                if ui
                    .add_enabled(
                        reviewer && b.status == RecordStatus::Pending,
                        egui::Button::new(label),
                    )
                    .clicked()
                {
                    self.run(Command::ConversionDecide {
                        id: b.terms.id.clone(),
                        version: b.version,
                        approve,
                        note: self.note.clone(),
                    });
                }
            }
            if ui
                .add_enabled(
                    owner && matches!(b.status, RecordStatus::Draft | RecordStatus::Pending),
                    egui::Button::new("Withdraw conversion book"),
                )
                .clicked()
            {
                self.run(Command::ConversionWithdraw {
                    id: b.terms.id.clone(),
                    version: b.version,
                });
            }
        });
        field(ui, "Export file", &mut self.export_path);
        if ui
            .add_enabled(
                b.status == RecordStatus::Approved,
                egui::Button::new("Export approved conversion evidence"),
            )
            .clicked()
        {
            let result = write_export(
                &self.export_path,
                copperline_engine::export_conversion(self.engine.state(), &b.terms.id),
            );
            self.result(result, "Approved conversion evidence exported.");
        }
    }
    pub(super) fn batch_replace_controls(&mut self, ui: &mut egui::Ui, q: &Quote) {
        ui.separator();
        ui.label("Stage multiple replacements to repair stale sources or change every line's currency in one save.");
        if ui
            .add_enabled(
                q.status == Status::Draft && q.owner == self.actor().name && !q.lines.is_empty(),
                egui::Button::new("Stage selected line"),
            )
            .clicked()
        {
            match self.request() {
                Ok(request) => {
                    let f = &mut self.conversion_form;
                    if f.batch_quote != q.id || f.batch_version != q.version {
                        f.batch.clear();
                        f.batch_quote = q.id.clone();
                        f.batch_version = q.version;
                    }
                    f.batch
                        .retain(|r| r.index != self.contract_form.replace_index);
                    f.batch.push(QuoteLineReplacement {
                        index: self.contract_form.replace_index,
                        request,
                    });
                }
                Err(e) => self.result::<()>(Err(e), ""),
            }
        }
        let f = &self.conversion_form;
        if f.batch_quote == q.id && !f.batch.is_empty() {
            ui.small(format!(
                "Staged against quote v{}; all changes commit or none do.",
                f.batch_version
            ));
            for r in &f.batch {
                ui.label(format!(
                    "Line {} · {} · quantity {} · {}",
                    r.index + 1,
                    r.request.product_id,
                    r.request.quantity,
                    r.request
                        .conversion
                        .as_ref()
                        .map(|c| format!("{} / {}", c.currency, c.unit))
                        .unwrap_or_else(|| "native currency / unit".into())
                ));
            }
            if primary(ui, "Apply staged replacements") {
                self.run(Command::QuoteReplaceLines {
                    id: q.id.clone(),
                    version: self.conversion_form.batch_version,
                    replacements: self.conversion_form.batch.clone(),
                });
                if !self.failed {
                    self.conversion_form.batch.clear();
                }
            }
            if ui.button("Clear staged replacements").clicked() {
                self.conversion_form.batch.clear();
            }
        }
    }
    pub(super) fn converted_reports_view(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new("Reporting currency · frozen actuals").default_open(true).show(ui,|ui|{
            ui.label("Convert recorded invoice and cost totals at each original invoice date. Approved reversals cancel those exact converted amounts. Quantity credits allocate from the converted whole original and restate its original invoice period. Quantities retain each recorded invoice unit; no unit aggregation.");
            ui.horizontal_wrapped(|ui|{field(ui,"Period from",&mut self.conversion_form.start);field(ui,"Until (exclusive)",&mut self.conversion_form.end);});
            ui.horizontal(|ui|{ui.label("Reporting currency");currency_picker(ui,"report-currency",&mut self.conversion_form.report_currency);field(ui,"New report ID",&mut self.conversion_form.report_id);});
            if primary(ui,"Freeze converted actuals") {
                let result=(||->Result<Command>{Ok(Command::ReportConvert{id:self.conversion_form.report_id.clone(),request:ReportingRequest{start:Date::parse(&self.conversion_form.start)?,end:Date::parse(&self.conversion_form.end)?,currency:self.conversion_form.report_currency}})})();
                match result {Ok(c)=>{self.run(c);if !self.failed{self.conversion_form.selected_report=self.conversion_form.report_id.clone();}},Err(e)=>self.result::<()>(Err(e),"")}
            }
            let reports:Vec<_>=self.engine.state().conversion_reports.values().cloned().collect();
            if self.conversion_form.selected_report.is_empty(){self.conversion_form.selected_report=reports.last().map(|r|r.id.clone()).unwrap_or_default();}
            egui::ComboBox::from_id_salt("frozen-report").selected_text(&self.conversion_form.selected_report).show_ui(ui,|ui|{for r in &reports{ui.selectable_value(&mut self.conversion_form.selected_report,r.id.clone(),&r.id);}});
            if let Some(r)=self.engine.state().conversion_reports.get(&self.conversion_form.selected_report).cloned() {
                let c=&r.calculation;ui.label(format!("{} · {} · [{} , {}) · source r{} · {} invoice rows",r.id,c.request.currency,c.request.start,c.request.end,c.source_revision,c.rows.len()));
                egui::Grid::new("report-summary").striped(true).spacing([22.0,10.0]).show(ui,|ui|{
                    for h in ["Customer","Gross invoices","Reversed","Credit notes","Fully credited invoices","Credited invoice / cost","Net invoice","Net cost","Net margin"] {ui.strong(h);}ui.end_row();
                    for g in &c.groups {ui.label(&g.customer_id);ui.label(g.gross_count.to_string());ui.label(g.reversal_count.to_string());ui.label(g.credit_count.to_string());ui.label(g.fully_credited_count.to_string());ui.label(format!("{} / {}",g.credited_invoice,g.credited_cost));ui.label(g.invoice.to_string());ui.label(g.cost.to_string());ui.label(g.margin.to_string());ui.end_row();}
                });
                egui::CollapsingHeader::new("Frozen invoice and rate evidence").show(ui, |ui| {
                    for row in &c.rows {
                        ui.label(format!(
                            "Recorded {} · {} {} / {} {} · {} invoice → {} {} · {}",
                            row.native.id, row.native.quantity, row.native.unit,
                            row.native.currency, row.native.invoice_total, row.native.date,
                            c.request.currency, row.invoice,
                            row.reversed_by.as_deref().unwrap_or("active")
                        ));
                        if let Some(b) = &row.booking {
                            ui.small(format!(
                                "Booking {} · quote {} v{} · line {} · booked r{} · nonposted pricing basis {} {} / {} {}",
                                b.booking_id, b.quote_id, b.quote_version,
                                b.line_index.saturating_add(1), b.booking_revision,
                                b.native_basis.quantity, b.native_basis.unit,
                                b.native_basis.currency, b.native_basis.invoice_total
                            ));
                        }
                        for credit in &row.credits {
                            ui.small(format!("Credit {} · {} {} · {} {} invoice / {} cost · approved r{}",
                                credit.credit_id, credit.quantity, row.native.unit, c.request.currency,
                                credit.invoice, credit.cost, credit.approved_revision));
                        }
                        if let Some(fx) = &row.fx {
                            ui.small(format!(
                                "{} × {}/{} · {} / {} v{} · [{} , {}) · approval r{}",
                                fx.entry.conversion.label(), fx.entry.ratio.numerator,
                                fx.entry.ratio.denominator, fx.book_id, fx.entry.id,
                                fx.book_version, fx.entry.start, fx.entry.end, fx.approval_revision
                            ));
                        }
                    }
                });
                field(ui,"Export file",&mut self.export_path);
                if ui.button("Export frozen report").clicked(){let result=write_export(&self.export_path,copperline_engine::export_report(self.engine.state(),&r.id));self.result(result,"Frozen reporting-currency actuals exported.");}
            }
            ui.small("Each recorded total rounds once, half away from zero; margin is converted invoice less converted cost. Reports do not post or settle money.");
        });
        ui.separator();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};
    #[test]
    fn native_conversion_review_quote_and_report_controls()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let mut engine = Engine::memory();
        engine.run(
            Actor {
                name: "seed".into(),
                role: Role::Admin,
            },
            Command::SeedDemo,
        )?;
        let mut app = Workbench::new(engine);
        app.view = View::Conversions;
        app.role = 1;
        let mut h = Harness::builder()
            .with_size(egui::vec2(1440.0, 1800.0))
            .build_ui_state(|ui, app: &mut Workbench| app.ui(ui), app);
        h.get_by_label("Prepare a conversion book").click();
        h.try_run().map_err(|e| e.to_string())?;
        h.get_by_label("Import draft conversion book").click();
        h.try_run().map_err(|e| e.to_string())?;
        assert!(!h.state().failed, "{}", h.state().status);
        h.get_by_label("Submit conversion book").click();
        h.try_run().map_err(|e| e.to_string())?;
        h.state_mut().role = 2;
        h.try_run().map_err(|e| e.to_string())?;
        h.get_by_label("Approve conversion book").click();
        h.try_run().map_err(|e| e.to_string())?;
        assert_eq!(
            h.state()
                .engine
                .state()
                .conversion_books
                .get("FX-UOM-OCT")
                .map(|b| b.status),
            Some(RecordStatus::Approved)
        );
        h.state_mut().view = View::Workbench;
        h.state_mut().role = 0;
        h.state_mut().select_conversion("EUR", "box")?;
        let v = h.state().live()?;
        assert_eq!(v.currency, Currency::EUR);
        assert_eq!(v.unit, "box");
        assert_eq!(
            v.conversion.as_ref().map(|c| c.native_quantity),
            Some(Quantity(120_000))
        );
        h.try_run().map_err(|e| e.to_string())?;
        h.get_by_label("Create quote").click();
        h.try_run().map_err(|e| e.to_string())?;
        h.get_by_label("Add selected product to quote").click();
        h.try_run().map_err(|e| e.to_string())?;
        assert!(!h.state().failed, "{}", h.state().status);
        h.state_mut().view = View::Analytics;
        h.state_mut().role = 1;
        h.try_run().map_err(|e| e.to_string())?;
        h.get_by_label("Freeze converted actuals").click();
        h.try_run().map_err(|e| e.to_string())?;
        assert!(!h.state().failed, "{}", h.state().status);
        assert_eq!(h.state().engine.state().conversion_reports.len(), 1);
        h.state().engine.state().validate()?;
        Ok(())
    }
}
