use super::*;
pub(super) struct ContractForm {
    pub(super) selected: String,
    pub(super) id: String,
    pub(super) name: String,
    pub(super) start: String,
    pub(super) end: String,
    pub(super) priority: String,
    pub(super) min_quantity: String,
    pub(super) max_quantity: String,
    pub(super) invoice_unit: String,
    pub(super) from_source: bool,
    pub(super) rows: Vec<ContractRow>,
    pub(super) replace_index: usize,
}
impl Default for ContractForm {
    fn default() -> Self {
        Self {
            selected: String::new(),
            id: "CP-ATLAS".into(),
            name: "Atlas contracted service prices".into(),
            start: "2026-10-01".into(),
            end: "2027-01-01".into(),
            priority: "100".into(),
            min_quantity: "1".into(),
            max_quantity: "10".into(),
            invoice_unit: "90.00".into(),
            from_source: false,
            rows: vec![],
            replace_index: 0,
        }
    }
}
impl Workbench {
    pub(super) fn contracts_view(&mut self, ui: &mut egui::Ui) {
        self.context(ui);
        ui.label("Approve customer price editions with explicit periods and quantity bands. Automatic selection uses the highest eligible priority; equal-priority overlaps are rejected. Original source prices and old quotes remain frozen.");
        egui::CollapsingHeader::new("Prepare a contract edition").default_open(self.engine.state().price_contracts.is_empty()).show(ui,|ui| {
            ui.horizontal_wrapped(|ui| {
                field(ui,"Family ID",&mut self.contract_form.id);
                ui.label(format!("Next edition {}",self.engine.state().latest_contract_edition(&self.contract_form.id).saturating_add(1)));
            });
            field(ui,"Name",&mut self.contract_form.name);
            ui.horizontal_wrapped(|ui| {field(ui,"Start inclusive",&mut self.contract_form.start);field(ui,"End exclusive",&mut self.contract_form.end);field(ui,"Priority",&mut self.contract_form.priority);});
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                egui::ComboBox::from_id_salt("contract-product").selected_text(&self.product).show_ui(ui,|ui| {
                    for p in self.engine.state().products.values() {ui.selectable_value(&mut self.product,p.id.clone(),format!("{} · {}",p.id,p.name));}
                });
                field(ui,"Minimum inclusive",&mut self.contract_form.min_quantity);
                field(ui,"Maximum exclusive (blank = none)",&mut self.contract_form.max_quantity);
            });
            ui.checkbox(&mut self.contract_form.from_source,"Freeze the selected approved grid/list invoice unit");
            if self.contract_form.from_source {self.source_picker(ui);ui.small("The top date is the origin snapshot date. The contract period above is separately approved. Origin discount/rebate are already included; no new discount/rebate is captured.");}
            else {field(ui,"Fixed invoice / unit",&mut self.contract_form.invoice_unit);}
            if ui.button("Add price band to draft").clicked() {self.add_contract_row();}
            let mut remove=None;
            for (i,r) in self.contract_form.rows.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(format!("{} · {} · {} <= quantity < {} · {}",r.id,r.product_id,r.min_quantity,
                        r.max_quantity.map(|q|q.to_string()).unwrap_or_else(||"unbounded".into()),
                        match &r.basis {ContractBasis::Fixed{invoice_unit}=>format!("{invoice_unit} / {}",r.unit),ContractBasis::ApprovedSource{..}=>"approved source".into()}));
                    if ui.small_button("Remove band").clicked(){remove=Some(i);}
                });
            }
            if let Some(i)=remove {self.contract_form.rows.remove(i);}
            if primary(ui,"Create immutable contract edition") {self.create_contract();}
            ui.small("Create freezes these terms. Corrections use a new edition; increasing editions may share the same start date. No FX or unit conversion is inferred.");
        });
        ui.add_space(16.0);
        let records: Vec<_> = self
            .engine
            .state()
            .price_contracts
            .iter()
            .map(|(k, c)| (k.clone(), c.clone()))
            .collect();
        if self.contract_form.selected.is_empty() {
            self.contract_form.selected = records
                .iter()
                .max_by_key(|(_, c)| c.created_revision)
                .map(|(k, _)| k.clone())
                .unwrap_or_default();
        }
        egui::Grid::new("contract-records")
            .striped(true)
            .spacing([18.0, 12.0])
            .show(ui, |ui| {
                for h in [
                    "Agreement",
                    "Edition",
                    "Customer",
                    "Period [start, end)",
                    "Priority",
                    "Status",
                ] {
                    ui.strong(h);
                }
                ui.end_row();
                for (key, c) in &records {
                    if ui
                        .selectable_label(self.contract_form.selected == *key, &c.terms.id)
                        .clicked()
                    {
                        self.contract_form.selected = key.clone();
                    }
                    ui.label(c.terms.edition.to_string());
                    ui.label(&c.terms.customer_id);
                    ui.label(format!("{} to {}", c.terms.start, c.terms.end));
                    ui.label(c.terms.priority.to_string());
                    ui.label(format!("{:?}", c.status));
                    ui.end_row();
                }
            });
        let Some(c) = self
            .engine
            .state()
            .price_contracts
            .get(&self.contract_form.selected)
            .cloned()
        else {
            return;
        };
        ui.add_space(14.0);
        ui.heading(format!("{} · edition {}", c.terms.name, c.terms.edition));
        ui.label(format!(
            "Owner {} · workflow v{} · created r{}",
            c.owner, c.version, c.created_revision
        ));
        ui.small(format!(
            "Reviewer exclusions: {}",
            c.contributors
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        ));
        egui::Grid::new("contract-price-rows")
            .striped(true)
            .spacing([22.0, 12.0])
            .show(ui, |ui| {
                for h in [
                    "Row",
                    "Product",
                    "Quantity [min, max)",
                    "Invoice / unit",
                    "Frozen origin",
                ] {
                    ui.strong(h);
                }
                ui.end_row();
                for row in &c.terms.rows {
                    if let Some(frozen) = c.rows.iter().find(|r| r.row_id == row.id) {
                        ui.label(&row.id);
                        ui.label(&row.product_id);
                        ui.label(format!(
                            "{} to {} {}",
                            row.min_quantity,
                            row.max_quantity
                                .map(|q| q.to_string())
                                .unwrap_or_else(|| "unbounded".into()),
                            row.unit
                        ));
                        ui.label(format!("{} {}", c.terms.currency, frozen.invoice_unit));
                        ui.label(
                            frozen
                                .origin
                                .as_ref()
                                .map(|s| {
                                    format!(
                                        "{} · captured r{}",
                                        s.source.label(),
                                        s.captured_revision
                                    )
                                })
                                .unwrap_or_else(|| "Fixed contract price".into()),
                        );
                        ui.end_row();
                    }
                }
            });
        if let Some(review) = &c.review {
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
        field(ui, "Review note", &mut self.note);
        let actor = self.actor();
        let owner = actor.name == c.owner && matches!(actor.role, Role::Analyst | Role::Admin);
        let reviewer = actor.role == Role::Approver && !c.contributors.contains(&actor.name);
        ui.horizontal_wrapped(|ui| {
            if ui.add_enabled(owner&&c.status==RecordStatus::Draft,egui::Button::new("Submit contract")).clicked(){self.run(Command::ContractSubmit{id:c.terms.id.clone(),edition:c.terms.edition,version:c.version});}
            for (label,approve) in [("Approve contract",true),("Reject contract",false)] {
                if ui.add_enabled(reviewer&&c.status==RecordStatus::Pending,egui::Button::new(label)).clicked(){self.run(Command::ContractDecide{id:c.terms.id.clone(),edition:c.terms.edition,version:c.version,approve,note:self.note.clone()});}
            }
            if ui.add_enabled(owner&&matches!(c.status,RecordStatus::Draft|RecordStatus::Pending),egui::Button::new("Withdraw contract")).clicked(){self.run(Command::ContractWithdraw{id:c.terms.id.clone(),edition:c.terms.edition,version:c.version});}
            if ui.add_enabled(owner,egui::Button::new("Prepare next edition")).clicked(){
                self.contract_form.id=c.terms.id.clone();self.contract_form.name=c.terms.name.clone();self.contract_form.start=c.terms.start.to_string();self.contract_form.end=c.terms.end.to_string();self.contract_form.priority=c.terms.priority.to_string();self.contract_form.rows=c.terms.rows.clone();self.customer=c.terms.customer_id.clone();
                self.status="Next edition copied into the preparation form; expand it to edit bands and create.".into();self.failed=false;
            }
        });
        ui.add_space(10.0);
        if ui
            .add_enabled(
                c.status == RecordStatus::Approved,
                egui::Button::new("Use automatic customer contracts in workbench"),
            )
            .clicked()
        {
            self.customer = c.terms.customer_id.clone();
            self.date = c.terms.start.to_string();
            if let Some(r) = c.terms.rows.first() {
                self.product = r.product_id.clone();
                self.quantity = r.min_quantity.to_string();
            }
            self.pricing_source = Some(PriceSource::CustomerContract);
            self.view = View::Workbench;
        }
        field(ui, "Export file", &mut self.export_path);
        if ui
            .add_enabled(
                c.status == RecordStatus::Approved,
                egui::Button::new("Export approved contract CSV"),
            )
            .clicked()
        {
            let result = (|| -> Result<()> {
                use std::io::Write;
                let csv = copperline_engine::export_contract(
                    self.engine.state(),
                    &c.terms.id,
                    c.terms.edition,
                )?;
                let mut file = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&self.export_path)
                    .map_err(|e| error(e.to_string()))?;
                file.write_all(csv.as_bytes())
                    .map_err(|e| error(e.to_string()))?;
                Ok(())
            })();
            self.result(
                result,
                "Approved contractual terms exported; later editions remain separate.",
            );
        }
    }
    fn add_contract_row(&mut self) {
        let result = (|| -> Result<ContractRow> {
            let product = self
                .engine
                .state()
                .products
                .get(&self.product)
                .ok_or_else(|| error("Select a product"))?;
            let minimum: Quantity = self.contract_form.min_quantity.parse()?;
            let basis = if self.contract_form.from_source {
                let mut request = self.request()?;
                request.quantity = minimum;
                request.discount = Bps(0);
                request.rebate = Bps(0);
                ContractBasis::ApprovedSource { request }
            } else {
                ContractBasis::Fixed {
                    invoice_unit: self.contract_form.invoice_unit.parse()?,
                }
            };
            let row_id = (1..=101)
                .map(|i| format!("ROW-{i}"))
                .find(|id| !self.contract_form.rows.iter().any(|r| &r.id == id))
                .ok_or_else(|| error("Too many rows"))?;
            Ok(ContractRow {
                id: row_id,
                product_id: product.id.clone(),
                unit: product.unit.clone(),
                min_quantity: minimum,
                max_quantity: if self.contract_form.max_quantity.is_empty() {
                    None
                } else {
                    Some(self.contract_form.max_quantity.parse()?)
                },
                basis,
            })
        })();
        match result {
            Ok(row) => {
                self.contract_form.rows.push(row);
                self.failed = false;
                self.status = "Price band added to preparation form.".into();
            }
            Err(e) => self.result::<()>(Err(e), ""),
        }
    }
    fn create_contract(&mut self) {
        let result = (|| -> Result<ContractTerms> {
            let product = self
                .engine
                .state()
                .products
                .get(&self.product)
                .ok_or_else(|| error("Select a product"))?;
            Ok(ContractTerms {
                id: self.contract_form.id.clone(),
                edition: self
                    .engine
                    .state()
                    .latest_contract_edition(&self.contract_form.id)
                    .checked_add(1)
                    .ok_or_else(|| error("Edition overflow"))?,
                name: self.contract_form.name.clone(),
                customer_id: self.customer.clone(),
                currency: product.currency,
                start: Date::parse(&self.contract_form.start)?,
                end: Date::parse(&self.contract_form.end)?,
                priority: self
                    .contract_form
                    .priority
                    .parse()
                    .map_err(|_| error("Priority must be an integer"))?,
                rows: self.contract_form.rows.clone(),
            })
        })();
        match result {
            Ok(terms) => {
                let key = contract_key(&terms.id, terms.edition);
                self.run(Command::ContractCreate { terms });
                if !self.failed {
                    self.contract_form.selected = key;
                    self.contract_form.rows.clear();
                }
            }
            Err(e) => self.result::<()>(Err(e), ""),
        }
    }
}
