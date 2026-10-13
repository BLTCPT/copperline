//! Replaceable native view. All business actions use Engine commands.
use copperline_engine::{Command, Engine, compare, evaluate, export_quote, totals};
use copperline_model::*;
use copperline_money::{Bps, Money, Quantity};
use eframe::egui::{self, Color32, RichText};
const NAVY: Color32 = Color32::from_rgb(24, 62, 84);
const COPPER: Color32 = Color32::from_rgb(172, 73, 35);
const TEAL: Color32 = Color32::from_rgb(24, 109, 99);
const SLATE: Color32 = Color32::from_rgb(83, 101, 117);
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum View {
    Workbench,
    Catalog,
    Rules,
    Lists,
    Quotes,
    Approvals,
    Scenarios,
    Audit,
}
impl View {
    fn label(self) -> &'static str {
        match self {
            Self::Workbench => "Price workbench",
            Self::Catalog => "Catalog & costs",
            Self::Rules => "Pricing rules",
            Self::Lists => "Price lists",
            Self::Quotes => "Quotes",
            Self::Approvals => "Approvals",
            Self::Scenarios => "Scenarios",
            Self::Audit => "Audit history",
        }
    }
}
pub struct Workbench {
    pub engine: Engine,
    pub view: View,
    role: usize,
    product: String,
    customer: String,
    date: String,
    quantity: String,
    discount: String,
    rebate: String,
    scenario_discount: String,
    selected_quote: String,
    selected_list: String,
    title: String,
    note: String,
    status: String,
    failed: bool,
    import_text: String,
    export_path: String,
    product_id: String,
    product_name: String,
    family: String,
    unit: String,
    list_price: String,
    cost: String,
    cost_date: String,
    rule_id: String,
    rule_name: String,
    rule_discount: String,
    rule_priority: String,
    rule_min_qty: String,
    rule_end: String,
    rule_segment: String,
}
impl Workbench {
    pub fn new(engine: Engine) -> Self {
        Self {
            engine,
            view: View::Workbench,
            role: 0,
            product: "VLV-100".into(),
            customer: "C-ATLAS".into(),
            date: "2026-10-13".into(),
            quantity: "10".into(),
            discount: "10".into(),
            rebate: "5".into(),
            scenario_discount: "15".into(),
            selected_quote: String::new(),
            selected_list: String::new(),
            title: "October service program".into(),
            note: "Reviewed against commercial guardrails".into(),
            status: "Ready. All demo data is synthetic.".into(),
            failed: false,
            import_text: "id,name,family,unit,currency,list_price\n".into(),
            export_path: "approved-quote.csv".into(),
            product_id: "VLV-100".into(),
            product_name: "Precision flow valve".into(),
            family: "Flow control".into(),
            unit: "each".into(),
            list_price: "100.00".into(),
            cost: "60.00".into(),
            cost_date: "2026-01-01".into(),
            rule_id: "CUSTOM-PRICE".into(),
            rule_name: "Customer negotiated tier".into(),
            rule_discount: "5".into(),
            rule_priority: "20".into(),
            rule_min_qty: "1".into(),
            rule_end: String::new(),
            rule_segment: String::new(),
        }
    }
    pub fn actor(&self) -> Actor {
        match self.role {
            1 => Actor {
                name: "Morgan / Pricing".into(),
                role: Role::Analyst,
            },
            2 => Actor {
                name: "Riley / Approval".into(),
                role: Role::Approver,
            },
            3 => Actor {
                name: "Casey / Admin".into(),
                role: Role::Admin,
            },
            _ => Actor {
                name: "Alex / Sales".into(),
                role: Role::Sales,
            },
        }
    }
    fn result<T>(&mut self, result: Result<T>, success: &str) {
        match result {
            Ok(_) => {
                self.status = success.into();
                self.failed = false;
            }
            Err(e) => {
                self.status = e.to_string();
                self.failed = true;
            }
        }
    }
    fn run(&mut self, command: Command) {
        let result = self.engine.run(self.actor(), command);
        match result {
            Ok(o) => {
                self.status = format!("{} · Saved revision {}", o.detail, o.revision);
                self.failed = false;
            }
            Err(e) => {
                self.status = e.to_string();
                self.failed = true;
            }
        }
    }
    fn request(&self) -> Result<PriceRequest> {
        Ok(PriceRequest {
            product_id: self.product.clone(),
            customer_id: self.customer.clone(),
            date: Date::parse(&self.date)?,
            quantity: self.quantity.parse()?,
            discount: self.discount.parse()?,
            rebate: self.rebate.parse()?,
        })
    }
    fn live(&self) -> Result<Evaluation> {
        evaluate(self.engine.state(), &self.request()?)
    }
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let mut style = (*ctx.global_style()).clone();
        style.visuals = egui::Visuals::light();
        style.visuals.selection.bg_fill = Color32::from_rgb(213, 232, 239);
        style.visuals.selection.stroke.color = NAVY;
        style.visuals.panel_fill = Color32::from_rgb(241, 245, 248);
        style.spacing.item_spacing = egui::vec2(10.0, 9.0);
        style
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(15.0));
        style
            .text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(14.0));
        ctx.set_global_style(style);
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(
                    if self.failed { Color32::DARK_RED } else { TEAL },
                    &self.status,
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.small(format!(
                        "Local workspace · r{}",
                        self.engine.state().revision
                    ));
                });
            });
        });
        egui::Panel::left("navigation")
            .exact_size(204.0)
            .frame(egui::Frame::new().fill(NAVY).inner_margin(18.0))
            .show(ui, |ui| {
                ui.visuals_mut().override_text_color = Some(Color32::WHITE);
                ui.add_space(12.0);
                ui.label(
                    RichText::new("Copperline")
                        .size(27.0)
                        .strong()
                        .color(Color32::WHITE),
                );
                ui.label(
                    RichText::new("Pricing workbench")
                        .color(Color32::from_rgb(182, 210, 222))
                        .size(13.0),
                );
                ui.add_space(28.0);
                for view in [
                    View::Workbench,
                    View::Catalog,
                    View::Rules,
                    View::Lists,
                    View::Quotes,
                    View::Approvals,
                    View::Scenarios,
                    View::Audit,
                ] {
                    let active = self.view == view;
                    let button = egui::Button::new(RichText::new(view.label()).color(if active {
                        NAVY
                    } else {
                        Color32::WHITE
                    }))
                    .fill(if active {
                        Color32::from_rgb(224, 238, 243)
                    } else {
                        Color32::TRANSPARENT
                    })
                    .min_size(egui::vec2(168.0, 37.0));
                    if ui.add(button).clicked() {
                        self.view = view;
                    }
                }
                ui.add_space(30.0);
                ui.label(
                    RichText::new("Synthetic workspace")
                        .size(12.0)
                        .color(Color32::from_rgb(182, 210, 222)),
                );
                ui.label(
                    RichText::new("USD · each\nNo enterprise connection")
                        .size(12.0)
                        .color(Color32::from_rgb(182, 210, 222)),
                );
            });
        egui::Panel::top("header")
            .frame(
                egui::Frame::new()
                    .fill(Color32::WHITE)
                    .inner_margin(egui::Margin::symmetric(24, 16)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading(RichText::new(self.view.label()).size(25.0).color(NAVY));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        egui::ComboBox::from_id_salt("role")
                            .selected_text(self.actor().name)
                            .show_ui(ui, |ui| {
                                for (i, label) in [
                                    "Alex / Sales",
                                    "Morgan / Pricing",
                                    "Riley / Approval",
                                    "Casey / Admin",
                                ]
                                .iter()
                                .enumerate()
                                {
                                    ui.selectable_value(&mut self.role, i, *label);
                                }
                            });
                        ui.small("Local demo identity");
                    });
                });
            });
        egui::CentralPanel::default().frame(egui::Frame::new().fill(Color32::from_rgb(241,245,248)).inner_margin(24.0)).show(ui,|ui|{egui::ScrollArea::both().auto_shrink([false,false]).show(ui,|ui|{
   if self.engine.state().products.is_empty(){ui.heading("Start a pricing workspace");ui.label("Load six synthetic industrial products, three customers, costs and a volume rule.");if ui.button("Load synthetic demo").clicked(){self.role=3;self.run(Command::SeedDemo);}return;}
   match self.view{View::Workbench=>self.workbench(ui),View::Catalog=>self.catalog(ui),View::Rules=>self.rules(ui),View::Lists=>self.lists(ui),View::Quotes=>self.quotes(ui),View::Approvals=>self.approvals(ui),View::Scenarios=>self.scenarios(ui),View::Audit=>self.audit(ui)}
  });});
    }
    fn context(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label("Customer");
            egui::ComboBox::from_id_salt("customer")
                .selected_text(
                    self.engine
                        .state()
                        .customers
                        .get(&self.customer)
                        .map(|c| c.name.as_str())
                        .unwrap_or("Choose customer"),
                )
                .width(300.0)
                .show_ui(ui, |ui| {
                    for c in self.engine.state().customers.values() {
                        ui.selectable_value(&mut self.customer, c.id.clone(), &c.name);
                    }
                });
            ui.label("Effective date");
            ui.add(egui::TextEdit::singleline(&mut self.date).desired_width(105.0));
            ui.label("Quantity");
            ui.add(egui::TextEdit::singleline(&mut self.quantity).desired_width(65.0));
        });
        ui.add_space(8.0);
    }
    fn workbench(&mut self, ui: &mut egui::Ui) {
        self.context(ui);
        ui.label(RichText::new("Live prices reflect the current catalog and effective rules. Select a product to inspect the full waterfall.").color(SLATE));
        ui.add_space(12.0);
        egui::Frame::new()
            .fill(Color32::WHITE)
            .inner_margin(16.0)
            .show(ui, |ui| {
                egui::Grid::new("live-grid")
                    .striped(true)
                    .min_col_width(88.0)
                    .spacing([20.0, 14.0])
                    .show(ui, |ui| {
                        for h in [
                            "Product",
                            "Product description",
                            "List / unit",
                            "Rule / unit",
                            "Cost / unit",
                            "Rule",
                        ] {
                            ui.strong(h);
                        }
                        ui.end_row();
                        let products: Vec<_> =
                            self.engine.state().products.values().cloned().collect();
                        for p in products {
                            if ui.selectable_label(self.product == p.id, &p.id).clicked() {
                                self.product = p.id.clone();
                            }
                            ui.label(&p.name);
                            ui.label(format!("{} {}", p.currency, p.list_price));
                            let r = self
                                .request()
                                .map(|mut r| {
                                    r.product_id = p.id.clone();
                                    r.discount = Bps(0);
                                    r.rebate = Bps(0);
                                    r
                                })
                                .and_then(|r| evaluate(self.engine.state(), &r));
                            match r {
                                Ok(v) => {
                                    ui.label(v.rule_unit.to_string());
                                    ui.label(v.cost_unit.to_string());
                                    ui.small(v.rule.unwrap_or_else(|| "Catalog list".into()));
                                }
                                Err(e) => {
                                    ui.colored_label(Color32::DARK_RED, "Unavailable");
                                    ui.label("—");
                                    ui.small(e.to_string());
                                }
                            }
                            ui.end_row();
                        }
                    });
            });
        ui.add_space(20.0);
        ui.columns(2, |columns| {
            if let Some(left) = columns.get_mut(0) {
                left.heading("Price explanation");
                left.horizontal(|ui| {
                    ui.label("Discount %");
                    ui.add(egui::TextEdit::singleline(&mut self.discount).desired_width(65.0));
                    ui.label("Rebate %");
                    ui.add(egui::TextEdit::singleline(&mut self.rebate).desired_width(65.0));
                });
                match self.live() {
                    Ok(v) => Self::waterfall(left, &v),
                    Err(e) => {
                        left.colored_label(Color32::DARK_RED, e.to_string());
                    }
                }
            }
            if let Some(right) = columns.get_mut(1) {
                right.heading("Build a quote");
                right.label("Freeze this calculation into a versioned commercial document.");
                field(right, "Quote title", &mut self.title);
                if primary(right, "Create quote") {
                    match Date::parse(&self.date) {
                        Ok(date) => {
                            let id = format!("Q-{:04}", self.engine.state().revision + 1);
                            self.run(Command::QuoteCreate {
                                id: id.clone(),
                                title: self.title.clone(),
                                customer_id: self.customer.clone(),
                                date,
                            });
                            if !self.failed {
                                self.selected_quote = id;
                            }
                        }
                        Err(e) => self.result::<()>(Err(e), ""),
                    }
                }
                self.quote_picker(right);
                if primary(right, "Add selected product to quote") {
                    self.add_selected();
                }
                if right.button("Open quote").clicked() {
                    self.view = View::Quotes;
                }
                if let Some(q) = self.engine.state().quotes.get(&self.selected_quote) {
                    right.label(format!(
                        "{} · {:?} · {} lines",
                        q.id,
                        q.status,
                        q.lines.len()
                    ));
                    if let Ok(t) = totals(&q.lines) {
                        right.label(
                            RichText::new(format!("{} {} pocket revenue", t.currency, t.pocket))
                                .size(21.0)
                                .color(NAVY),
                        );
                    }
                }
            }
        });
    }
    fn waterfall(ui: &mut egui::Ui, v: &Evaluation) {
        ui.label(
            RichText::new(format!(
                "{} · {} {}",
                v.product_name, v.request.quantity, v.unit
            ))
            .strong(),
        );
        egui::Grid::new("waterfall-values")
            .num_columns(2)
            .spacing([24.0, 8.0])
            .show(ui, |ui| {
                for (label, value) in [
                    ("List / unit", v.list_unit),
                    ("After selected rule / unit", v.rule_unit),
                    ("Invoice / unit", v.invoice_unit),
                    ("Invoice total", v.invoice_total),
                    ("Rebate accrual", v.rebate_total),
                    ("Pocket revenue", v.pocket_total),
                    ("Cost", v.cost_total),
                    ("Pocket margin", v.margin_total),
                ] {
                    ui.label(label);
                    ui.label(RichText::new(format!("{} {}", v.currency, value)).strong());
                    ui.end_row();
                }
            });
        ui.add_space(8.0);
        let width = ui.available_width().min(520.0);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 32.0), egui::Sense::hover());
        let total = v.invoice_total.0.max(1) as f32;
        let mut x = rect.left();
        for (amount, color) in [
            (v.cost_total.0, SLATE),
            (v.margin_total.0.max(0), TEAL),
            (v.rebate_total.0, COPPER),
        ] {
            let w = (amount as f32 / total * width).clamp(0.0, width);
            ui.painter().rect_filled(
                egui::Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(w, 32.0)),
                0,
                color,
            );
            x += w;
        }
        ui.horizontal_wrapped(|ui| {
            ui.colored_label(SLATE, "Cost");
            ui.colored_label(TEAL, "Pocket margin");
            ui.colored_label(COPPER, "Rebate");
        });
        ui.label(format!(
            "Pocket margin: {}",
            v.margin_bps
                .map(|b| format!("{}%", Bps(b)))
                .unwrap_or_else(|| "Undefined".into())
        ));
        ui.small(format!(
            "{} · Cost v{}",
            v.rule
                .as_deref()
                .unwrap_or("Catalog list; no matching rule"),
            v.cost_version
        ));
        for w in &v.warnings {
            ui.colored_label(COPPER, w);
        }
        ui.small("Rebate rounds once on the extended invoice. Units use 3 decimals; money uses 2.");
    }
    fn quote_picker(&mut self, ui: &mut egui::Ui) {
        egui::ComboBox::from_id_salt("quote-picker")
            .selected_text(if self.selected_quote.is_empty() {
                "Choose quote"
            } else {
                &self.selected_quote
            })
            .show_ui(ui, |ui| {
                for q in self.engine.state().quotes.values() {
                    ui.selectable_value(
                        &mut self.selected_quote,
                        q.id.clone(),
                        format!("{} · {:?}", q.id, q.status),
                    );
                }
            });
    }
    fn add_selected(&mut self) {
        let r = self.request();
        let version = self
            .engine
            .state()
            .quotes
            .get(&self.selected_quote)
            .map(|q| q.version);
        match (r, version) {
            (Ok(request), Some(version)) => self.run(Command::QuoteAddLine {
                id: self.selected_quote.clone(),
                version,
                request,
            }),
            (Err(e), _) => self.result::<()>(Err(e), ""),
            _ => self.result::<()>(Err(error("Create or select a quote first")), ""),
        }
    }
    fn catalog(&mut self, ui: &mut egui::Ui) {
        ui.label("Products have immutable currency and unit. Costs are effective-dated, append-only versions.");
        ui.add_space(12.0);
        egui::Grid::new("catalog")
            .striped(true)
            .spacing([20.0, 10.0])
            .show(ui, |ui| {
                for h in ["Product", "Name", "Family", "List price", "Unit"] {
                    ui.strong(h);
                }
                ui.end_row();
                let products: Vec<_> = self.engine.state().products.values().cloned().collect();
                for p in products {
                    if ui
                        .selectable_label(self.product_id == p.id, &p.id)
                        .clicked()
                    {
                        self.product_id = p.id.clone();
                        self.product_name = p.name.clone();
                        self.family = p.family.clone();
                        self.unit = p.unit.clone();
                        self.list_price = p.list_price.to_string();
                        if let Some(c) = self
                            .engine
                            .state()
                            .costs
                            .iter()
                            .filter(|c| c.product_id == p.id)
                            .max_by_key(|c| c.version)
                        {
                            self.cost = c.amount.to_string();
                            self.cost_date = c.effective_from.to_string();
                        }
                    }
                    ui.label(p.name);
                    ui.label(p.family);
                    ui.label(format!("{} {}", p.currency, p.list_price));
                    ui.label(p.unit);
                    ui.end_row();
                }
            });
        ui.separator();
        ui.columns(2, |cols| {
            if let Some(ui) = cols.get_mut(0) {
                ui.heading("Product details");
                field(ui, "Product ID", &mut self.product_id);
                field(ui, "Name", &mut self.product_name);
                field(ui, "Family", &mut self.family);
                field(ui, "Unit", &mut self.unit);
                field(ui, "List price · USD", &mut self.list_price);
                if primary(ui, "Save product") {
                    match self.list_price.parse::<Money>() {
                        Ok(list_price) => self.run(Command::ProductPut(Product {
                            id: self.product_id.clone(),
                            name: self.product_name.clone(),
                            family: self.family.clone(),
                            unit: self.unit.clone(),
                            currency: Currency::USD,
                            list_price,
                        })),
                        Err(e) => self.result::<()>(Err(e), ""),
                    }
                }
            }
            if let Some(ui) = cols.get_mut(1) {
                ui.heading("Cost input");
                ui.label(format!("For product {}", self.product_id));
                field(ui, "Unit cost · USD", &mut self.cost);
                field(ui, "Effective from", &mut self.cost_date);
                if primary(ui, "Add cost version") {
                    let r = (|| {
                        Ok(Cost {
                            product_id: self.product_id.clone(),
                            version: self
                                .engine
                                .state()
                                .costs
                                .iter()
                                .filter(|c| c.product_id == self.product_id)
                                .map(|c| c.version)
                                .max()
                                .unwrap_or(0)
                                + 1,
                            effective_from: Date::parse(&self.cost_date)?,
                            amount: self.cost.parse()?,
                            currency: Currency::USD,
                            unit: self.unit.clone(),
                        })
                    })();
                    match r {
                        Ok(cost) => self.run(Command::CostAdd(cost)),
                        Err(e) => self.result::<()>(Err(e), ""),
                    }
                }
                ui.label("A missing cost blocks pricing; zero is an explicit valid cost.");
            }
        });
        ui.separator();
        ui.collapsing("Import new products from CSV",|ui|{ui.label("Paste CSV with the shown header. Duplicate or invalid rows reject the whole import.");ui.add(egui::TextEdit::multiline(&mut self.import_text).desired_rows(6).desired_width(f32::INFINITY));if ui.button("Validate and import products").clicked(){self.run(Command::ProductsImport{csv:self.import_text.clone()});}});
    }
    fn rules(&mut self, ui: &mut egui::Ui) {
        ui.label("Exactly one rule wins: priority, specificity, effective start, version, then rule ID (descending). New versions supersede the same rule after their start date. End dates are exclusive.");
        egui::Grid::new("rules")
            .striped(true)
            .spacing([20.0, 12.0])
            .show(ui, |ui| {
                for h in [
                    "Rule",
                    "Version",
                    "Priority",
                    "Start",
                    "Minimum qty",
                    "Action",
                ] {
                    ui.strong(h);
                }
                ui.end_row();
                for r in &self.engine.state().rules {
                    ui.label(&r.name);
                    ui.label(r.version.to_string());
                    ui.label(r.priority.to_string());
                    ui.label(r.effective_from.to_string());
                    ui.label(r.min_quantity.to_string());
                    ui.label(format!("{:?}", r.action));
                    ui.end_row();
                }
            });
        ui.separator();
        ui.heading("Add a versioned discount rule");
        ui.label("Applies to the product and customer selected in the price workbench. Rules never mutate past snapshots.");
        ui.label(format!("{} / {}", self.product, self.customer));
        field(ui, "Rule ID", &mut self.rule_id);
        field(ui, "Name", &mut self.rule_name);
        field(ui, "Priority", &mut self.rule_priority);
        field(ui, "Discount %", &mut self.rule_discount);
        field(ui, "Minimum quantity", &mut self.rule_min_qty);
        field(ui, "Effective start", &mut self.date);
        field(ui, "Exclusive end (optional)", &mut self.rule_end);
        field(ui, "Segment (optional)", &mut self.rule_segment);
        if primary(ui, "Add rule version") {
            let result = (|| {
                Ok(Rule {
                    id: self.rule_id.clone(),
                    version: self
                        .engine
                        .state()
                        .rules
                        .iter()
                        .filter(|r| r.id == self.rule_id)
                        .map(|r| r.version)
                        .max()
                        .unwrap_or(0)
                        + 1,
                    name: self.rule_name.clone(),
                    priority: self
                        .rule_priority
                        .parse()
                        .map_err(|_| error("Priority must be a whole number"))?,
                    product_id: Some(self.product.clone()),
                    customer_id: Some(self.customer.clone()),
                    segment: if self.rule_segment.is_empty() {
                        None
                    } else {
                        Some(self.rule_segment.clone())
                    },
                    effective_from: Date::parse(&self.date)?,
                    effective_to: if self.rule_end.is_empty() {
                        None
                    } else {
                        Some(Date::parse(&self.rule_end)?)
                    },
                    min_quantity: self.rule_min_qty.parse()?,
                    action: RuleAction::Discount(self.rule_discount.parse()?),
                    currency: Currency::USD,
                })
            })();
            match result {
                Ok(rule) => self.run(Command::RuleAdd(rule)),
                Err(e) => self.result::<()>(Err(e), ""),
            }
        }
        ui.add_space(12.0);
        ui.heading("Guardrails");
        let p = &self.engine.state().policy;
        ui.label(format!(
            "Approval margin floor {}% · Maximum effective discount {}% · Hard margin floor {}%",
            p.min_margin, p.max_discount, p.hard_min_margin
        ));
        ui.small("All quotes require a separate approval in this demo. Hard-floor violations block submission.");
    }
    fn lists(&mut self, ui: &mut egui::Ui) {
        self.context(ui);
        ui.label("Calculate a point-in-time list, submit the entire list, approve with another person, then publish locally. Live changes never alter an existing list.");
        field(ui, "List name", &mut self.title);
        if primary(ui, "Calculate price list") {
            let result = (|| Ok((Date::parse(&self.date)?, self.quantity.parse::<Quantity>()?)))();
            match result {
                Ok((date, quantity)) => {
                    let id = format!("PL-{:04}", self.engine.state().revision + 1);
                    self.run(Command::ListCalculate {
                        id: id.clone(),
                        name: self.title.clone(),
                        customer_id: self.customer.clone(),
                        date,
                        quantity,
                        currency: Currency::USD,
                    });
                    if !self.failed {
                        self.selected_list = id;
                    }
                }
                Err(e) => self.result::<()>(Err(e), ""),
            }
        }
        egui::Grid::new("list-table")
            .striped(true)
            .spacing([20.0, 12.0])
            .show(ui, |ui| {
                for h in ["List", "Name", "Status", "Source revision", "Items"] {
                    ui.strong(h);
                }
                ui.end_row();
                for l in self.engine.state().price_lists.values() {
                    if ui
                        .selectable_label(self.selected_list == l.id, &l.id)
                        .clicked()
                    {
                        self.selected_list = l.id.clone();
                    }
                    ui.label(&l.name);
                    ui.label(format!("{:?}", l.status));
                    ui.label(l.source_revision.to_string());
                    ui.label(l.lines.len().to_string());
                    ui.end_row();
                }
            });
        if let Some(l) = self
            .engine
            .state()
            .price_lists
            .get(&self.selected_list)
            .cloned()
        {
            ui.separator();
            ui.heading(format!("{} · {:?}", l.id, l.status));
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(l.status == Status::Draft, egui::Button::new("Submit list"))
                    .clicked()
                {
                    self.run(Command::ListSubmit {
                        id: l.id.clone(),
                        version: l.version,
                    });
                }
                if ui
                    .add_enabled(
                        l.status == Status::Approved,
                        egui::Button::new("Publish approved list locally"),
                    )
                    .clicked()
                {
                    self.run(Command::ListPublish {
                        id: l.id.clone(),
                        version: l.version,
                    });
                }
            });
            Self::lines_table(ui, &l.lines);
        }
    }
    fn lines_table(ui: &mut egui::Ui, lines: &[Evaluation]) {
        egui::Grid::new("document-lines")
            .striped(true)
            .spacing([20.0, 12.0])
            .show(ui, |ui| {
                for h in [
                    "Product", "Quantity", "Invoice", "Rebate", "Pocket", "Margin",
                ] {
                    ui.strong(h);
                }
                ui.end_row();
                for l in lines {
                    ui.label(&l.request.product_id);
                    ui.label(l.request.quantity.to_string());
                    ui.label(l.invoice_total.to_string());
                    ui.label(l.rebate_total.to_string());
                    ui.label(l.pocket_total.to_string());
                    ui.label(l.margin_total.to_string());
                    ui.end_row();
                }
            });
        if let Ok(t) = totals(lines) {
            ui.add_space(12.0);
            ui.label(
                RichText::new(format!(
                    "{} {} pocket revenue    {} margin    {}% weighted margin",
                    t.currency,
                    t.pocket,
                    t.margin,
                    t.margin_bps
                        .map(|b| Bps(b).to_string())
                        .unwrap_or_else(|| "undefined".into())
                ))
                .size(20.0)
                .color(NAVY),
            );
        }
    }
    fn quotes(&mut self, ui: &mut egui::Ui) {
        self.quote_picker(ui);
        let Some(q) = self
            .engine
            .state()
            .quotes
            .get(&self.selected_quote)
            .cloned()
        else {
            ui.label("Create a quote from the price workbench, then return here to review and submit it.");
            return;
        };
        ui.heading(&q.title);
        ui.label(format!(
            "{} · {:?} · v{} · Pricing data r{} · {}",
            q.id, q.status, q.version, q.source_revision, q.date
        ));
        ui.add_space(10.0);
        Self::lines_table(ui, &q.lines);
        for l in &q.lines {
            for w in &l.warnings {
                ui.colored_label(COPPER, format!("{}: {w}", l.request.product_id));
            }
        }
        ui.add_space(16.0);
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    q.status == Status::Draft,
                    egui::Button::new("Recalculate draft"),
                )
                .clicked()
            {
                self.run(Command::QuoteReprice {
                    id: q.id.clone(),
                    version: q.version,
                });
            }
            if ui
                .add_enabled(
                    q.status == Status::Draft && !q.lines.is_empty(),
                    egui::Button::new("Remove last line"),
                )
                .clicked()
            {
                self.run(Command::QuoteRemoveLine {
                    id: q.id.clone(),
                    version: q.version,
                    index: q.lines.len().saturating_sub(1),
                });
            }
            if ui
                .add_enabled(
                    q.status == Status::Draft,
                    egui::Button::new("Submit for approval"),
                )
                .clicked()
            {
                self.run(Command::QuoteSubmit {
                    id: q.id.clone(),
                    version: q.version,
                });
            }
            if ui
                .add_enabled(
                    matches!(q.status, Status::Approved | Status::Rejected),
                    egui::Button::new("Revise quote"),
                )
                .clicked()
            {
                self.run(Command::QuoteRevise {
                    id: q.id.clone(),
                    version: q.version,
                });
            }
        });
        ui.separator();
        field(ui, "Export file", &mut self.export_path);
        if ui
            .add_enabled(
                q.status == Status::Approved,
                egui::Button::new("Export approved CSV"),
            )
            .clicked()
        {
            let result = (|| {
                use std::io::Write;
                let csv = export_quote(&q)?;
                let mut file = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&self.export_path)
                    .map_err(|e| error(format!("Choose a new export path: {e}")))?;
                file.write_all(csv.as_bytes())
                    .map_err(|e| error(e.to_string()))?;
                Ok(())
            })();
            self.result(
                result,
                "Approved quote exported. Snapshot source revision is included.",
            );
        }
        ui.small("Exports use frozen approved figures. Existing files are never overwritten.");
        for d in &q.decisions {
            ui.label(format!(
                "{} · {} · {}",
                d.actor,
                if d.approved { "Approved" } else { "Rejected" },
                d.note
            ));
        }
    }
    fn approvals(&mut self, ui: &mut egui::Ui) {
        ui.label("Pending documents are immutable. An approver must be different from the owner and submitter.");
        field(ui, "Decision note", &mut self.note);
        let quotes: Vec<_> = self
            .engine
            .state()
            .quotes
            .values()
            .filter(|q| q.status == Status::Pending)
            .cloned()
            .collect();
        let lists: Vec<_> = self
            .engine
            .state()
            .price_lists
            .values()
            .filter(|l| l.status == Status::Pending)
            .cloned()
            .collect();
        if quotes.is_empty() && lists.is_empty() {
            ui.label("No documents awaiting approval. Submit a draft quote or price list first.");
        }
        for q in quotes {
            ui.separator();
            ui.heading(format!("Quote {} · {}", q.id, q.title));
            ui.label(format!(
                "Owner {} · Version {} · Source r{}",
                q.owner, q.version, q.source_revision
            ));
            Self::lines_table(ui, &q.lines);
            for l in &q.lines {
                for w in &l.warnings {
                    ui.colored_label(COPPER, w);
                }
            }
            ui.horizontal(|ui| {
                if primary(ui, "Approve quote") {
                    self.run(Command::QuoteDecide {
                        id: q.id.clone(),
                        version: q.version,
                        approve: true,
                        note: self.note.clone(),
                    });
                }
                if ui.button("Reject quote").clicked() {
                    self.run(Command::QuoteDecide {
                        id: q.id.clone(),
                        version: q.version,
                        approve: false,
                        note: self.note.clone(),
                    });
                }
            });
        }
        for l in lists {
            ui.separator();
            ui.heading(format!("Price list {} · {}", l.id, l.name));
            Self::lines_table(ui, &l.lines);
            ui.horizontal(|ui| {
                if primary(ui, "Approve list") {
                    self.run(Command::ListDecide {
                        id: l.id.clone(),
                        version: l.version,
                        approve: true,
                        note: self.note.clone(),
                    });
                }
                if ui.button("Reject list").clicked() {
                    self.run(Command::ListDecide {
                        id: l.id.clone(),
                        version: l.version,
                        approve: false,
                        note: self.note.clone(),
                    });
                }
            });
        }
    }
    fn scenarios(&mut self, ui: &mut egui::Ui) {
        self.context(ui);
        ui.heading(format!("{} · Discount sensitivity", self.product));
        ui.label("Projected commercial outcomes for the same product and customer. No demand prediction or historical actuals are implied.");
        field(ui, "Baseline discount %", &mut self.discount);
        field(ui, "Scenario discount %", &mut self.scenario_discount);
        let result = (|| {
            let baseline = self.request()?;
            let mut scenario = baseline.clone();
            scenario.discount = self.scenario_discount.parse()?;
            compare(self.engine.state(), &baseline, &scenario)
        })();
        match result {
            Ok(c) => {
                ui.columns(2, |cols| {
                    if let Some(ui) = cols.get_mut(0) {
                        ui.heading("Baseline");
                        Self::waterfall(ui, &c.baseline);
                    }
                    if let Some(ui) = cols.get_mut(1) {
                        ui.heading("Scenario");
                        Self::waterfall(ui, &c.scenario);
                    }
                });
                ui.separator();
                ui.label(
                    RichText::new(format!(
                        "Pocket revenue change: {} {}    Margin change: {} {}",
                        c.baseline.currency, c.pocket_delta, c.baseline.currency, c.margin_delta
                    ))
                    .strong(),
                );
            }
            Err(e) => {
                ui.colored_label(Color32::DARK_RED, e.to_string());
            }
        }
    }
    fn audit(&self, ui: &mut egui::Ui) {
        ui.label("Every successful business command has a durable revision. Failed commands leave no partial changes. Actor labels are local demo identities, not authenticated users.");
        ui.add_space(12.0);
        egui::Grid::new("audit")
            .striped(true)
            .spacing([20.0, 12.0])
            .show(ui, |ui| {
                for h in ["Revision", "Actor", "Action", "Result"] {
                    ui.strong(h);
                }
                ui.end_row();
                for a in self.engine.state().audit.iter().rev().take(200) {
                    ui.label(a.revision.to_string());
                    ui.label(&a.actor.name);
                    ui.label(&a.command);
                    ui.label(&a.detail);
                    ui.end_row();
                }
            });
    }
}
fn field(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.horizontal(|ui| {
        ui.add_sized([155.0, 24.0], egui::Label::new(label));
        ui.add(egui::TextEdit::singleline(value).desired_width(245.0));
    });
}
fn primary(ui: &mut egui::Ui, label: &str) -> bool {
    ui.add(
        egui::Button::new(RichText::new(label).color(Color32::WHITE))
            .fill(COPPER)
            .min_size(egui::vec2(120.0, 32.0)),
    )
    .clicked()
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};
    fn harness() -> Result<Harness<'static, Workbench>> {
        let mut engine = Engine::memory();
        engine.run(
            Actor {
                name: "Admin".into(),
                role: Role::Admin,
            },
            Command::SeedDemo,
        )?;
        Ok(Harness::builder()
            .with_size(egui::vec2(1440.0, 1200.0))
            .build_ui_state(|ui, app: &mut Workbench| app.ui(ui), Workbench::new(engine)))
    }
    #[test]
    fn native_quote_buttons_complete_approval_flow()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let mut h = harness()?;
        h.get_by_label("Create quote").click();
        h.try_run().map_err(|e| e.to_string())?;
        assert_eq!(h.state().engine.state().quotes.len(), 1);
        h.get_by_label("Add selected product to quote").click();
        h.try_run().map_err(|e| e.to_string())?;
        let id = h.state().selected_quote.clone();
        assert_eq!(
            h.state()
                .engine
                .state()
                .quotes
                .get(&id)
                .map(|q| q.lines.len()),
            Some(1)
        );
        h.get_by_label("Open quote").click();
        h.try_run().map_err(|e| e.to_string())?;
        h.get_by_label("Submit for approval").click();
        h.try_run().map_err(|e| e.to_string())?;
        assert_eq!(
            h.state().engine.state().quotes.get(&id).map(|q| q.status),
            Some(Status::Pending)
        );
        h.state_mut().role = 2;
        h.state_mut().view = View::Approvals;
        h.try_run().map_err(|e| e.to_string())?;
        h.get_by_label("Approve quote").click();
        h.try_run().map_err(|e| e.to_string())?;
        assert_eq!(
            h.state().engine.state().quotes.get(&id).map(|q| q.status),
            Some(Status::Approved)
        );
        Ok(())
    }
    #[test]
    fn unauthorized_ui_action_shows_error_without_mutation()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let mut h = harness()?;
        let revision = h.state().engine.state().revision;
        h.state_mut().view = View::Catalog;
        h.try_run().map_err(|e| e.to_string())?;
        h.get_by_label("Save product").click();
        h.try_run().map_err(|e| e.to_string())?;
        assert!(h.state().failed);
        assert!(h.state().status.contains("Role is not allowed"));
        assert_eq!(h.state().engine.state().revision, revision);
        Ok(())
    }
    #[test]
    fn every_workspace_view_renders_at_minimum_size()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let mut h = harness()?;
        h.set_size(egui::vec2(1050.0, 720.0));
        for view in [
            View::Workbench,
            View::Catalog,
            View::Rules,
            View::Lists,
            View::Quotes,
            View::Approvals,
            View::Scenarios,
            View::Audit,
        ] {
            h.state_mut().view = view;
            h.try_run().map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}
