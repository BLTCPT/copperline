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
    LiveGrid,
    Lists,
    Quotes,
    Approvals,
    Agreements,
    Analytics,
    Scenarios,
    Governance,
    Audit,
}
impl View {
    fn label(self) -> &'static str {
        match self {
            Self::Workbench => "Price workbench",
            Self::Catalog => "Catalog & costs",
            Self::Rules => "Pricing rules",
            Self::LiveGrid => "Live price grid",
            Self::Lists => "Price lists",
            Self::Quotes => "Quotes",
            Self::Approvals => "Approvals",
            Self::Agreements => "Agreements & rebates",
            Self::Analytics => "Invoice analytics",
            Self::Scenarios => "Scenarios",
            Self::Governance => "Approval policy",
            Self::Audit => "Audit history",
        }
    }
}
pub struct Workbench {
    pub engine: Engine,
    pub view: View,
    role: usize,
    pricing_source: Option<PriceSource>,
    approval_discount: String,
    approval_margin: String,
    approval_usd: String,
    approval_eur: String,
    approval_gbp: String,
    product: String,
    customer: String,
    date: String,
    quantity: String,
    discount: String,
    rebate: String,
    scenario_discount: String,
    selected_quote: String,
    selected_list: String,
    selected_grid: String,
    selected_agreement: String,
    agreement_end: String,
    tier_threshold: String,
    base_rate: String,
    tier_rate: String,
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
        let thresholds = engine
            .state()
            .quote_approval_policies
            .last()
            .map(|policy| policy.thresholds.clone())
            .unwrap_or_default();
        let selected_quote = engine
            .state()
            .quotes
            .keys()
            .next()
            .cloned()
            .unwrap_or_default();
        let selected_list = engine
            .state()
            .price_lists
            .keys()
            .next()
            .cloned()
            .unwrap_or_default();
        let selected_grid = engine
            .state()
            .grid
            .keys()
            .next()
            .cloned()
            .unwrap_or_default();
        let selected_agreement = engine
            .state()
            .agreements
            .keys()
            .next()
            .cloned()
            .unwrap_or_default();
        Self {
            engine,
            view: View::Workbench,
            role: 0,
            pricing_source: None,
            approval_discount: thresholds.max_discount.to_string(),
            approval_margin: thresholds.min_margin.to_string(),
            approval_usd: thresholds.usd_value.to_string(),
            approval_eur: thresholds.eur_value.to_string(),
            approval_gbp: thresholds.gbp_value.to_string(),
            product: "VLV-100".into(),
            customer: "C-ATLAS".into(),
            date: "2026-10-13".into(),
            quantity: "10".into(),
            discount: "10".into(),
            rebate: "5".into(),
            scenario_discount: "15".into(),
            selected_quote,
            selected_list,
            selected_grid,
            selected_agreement,
            agreement_end: "2027-01-01".into(),
            tier_threshold: "1000.00".into(),
            base_rate: "2".into(),
            tier_rate: "5".into(),
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
            4 => Actor {
                name: "Jordan / Finance".into(),
                role: Role::Finance,
            },
            _ => Actor {
                name: "Alex / Sales".into(),
                role: Role::Sales,
            },
        }
    }
    pub fn select_pricing_source(&mut self, source: PriceSource) -> Result<()> {
        let mut request = self.request()?;
        request.source = Some(source.clone());
        evaluate(self.engine.state(), &request)?;
        self.pricing_source = Some(source);
        Ok(())
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
            source: self.pricing_source.clone(),
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
                egui::ScrollArea::vertical().show(ui, |ui| {
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
                        View::LiveGrid,
                        View::Lists,
                        View::Quotes,
                        View::Approvals,
                        View::Agreements,
                        View::Analytics,
                        View::Scenarios,
                        View::Governance,
                        View::Audit,
                    ] {
                        let active = self.view == view;
                        let button =
                            egui::Button::new(RichText::new(view.label()).color(if active {
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
                                    "Jordan / Finance",
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
   match self.view{View::Workbench=>self.workbench(ui),View::Catalog=>self.catalog(ui),View::Rules=>self.rules(ui),View::LiveGrid=>self.live_grid(ui),View::Lists=>self.lists(ui),View::Quotes=>self.quotes(ui),View::Approvals=>self.approvals(ui),View::Agreements=>self.agreements(ui),View::Analytics=>self.analytics_view(ui),View::Scenarios=>self.scenarios(ui),View::Governance=>self.governance(ui),View::Audit=>self.audit(ui)}
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
        self.source_picker(ui);
        ui.label(RichText::new("The catalog table uses current rules. Choose an explicit pricing basis for the selected product and inspect its full waterfall.").color(SLATE));
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
                                    r.source = None;
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
                    (
                        if v.source.is_some() {
                            "Approved source / unit"
                        } else {
                            "After selected rule / unit"
                        },
                        v.rule_unit,
                    ),
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
            v.source
                .as_ref()
                .map(|s| s.source.label())
                .unwrap_or_else(|| v
                    .rule
                    .clone()
                    .unwrap_or_else(|| "Catalog list; no matching rule".into())),
            v.cost_version
        ));
        if let Some(source) = &v.source {
            ui.colored_label(TEAL, source.source.label());
            ui.small(format!(
                "Source approval r{} · captured r{}",
                source.approval_revision, source.captured_revision
            ));
        }
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
        Self::quote_route(ui, &q);
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
        ui.label("Pending documents are immutable. Quote stages exclude every contributor and require a different person at each stage.");
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
            Self::quote_route(ui, &q);
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
        self.source_picker(ui);
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
impl Workbench {
    fn source_picker(&mut self, ui: &mut egui::Ui) {
        let mut choices: Vec<PriceSource> = vec![];
        for item in self.engine.state().grid.values() {
            if let Some(active) = &item.last_approved
                && active.evaluation.request.product_id == self.product
                && active.evaluation.request.customer_id == self.customer
            {
                choices.push(PriceSource::Grid {
                    id: item.id.clone(),
                    approval_revision: active.decision.revision,
                });
            }
        }
        for list in self.engine.state().price_lists.values() {
            if list.status == Status::Published
                && list.customer_id == self.customer
                && list
                    .lines
                    .iter()
                    .any(|line| line.request.product_id == self.product)
            {
                choices.push(PriceSource::PriceList {
                    id: list.id.clone(),
                    version: list.version,
                });
            }
        }
        ui.horizontal_wrapped(|ui| {
            ui.label("Quote / scenario pricing basis");
            egui::ComboBox::from_id_salt("pricing-source")
                .selected_text(
                    self.pricing_source
                        .as_ref()
                        .map(PriceSource::label)
                        .unwrap_or_else(|| "Live catalog and effective rules".into()),
                )
                .width(360.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.pricing_source,
                        None,
                        "Live catalog and effective rules",
                    );
                    for source in choices {
                        let label = source.label();
                        ui.selectable_value(&mut self.pricing_source, Some(source), label);
                    }
                });
        });
        if self.pricing_source.is_some() {
            ui.small("Approved sources require the exact pricing date and minimum quantity. Stale selections fail explicitly.");
        }
    }
    fn quote_route(ui: &mut egui::Ui, quote: &Quote) {
        ui.small(format!(
            "Owner: {}. Owner-only editing and submission; no Admin bypass.",
            quote.owner
        ));
        if let Some(governance) = &quote.governance {
            ui.small(format!(
                "Excluded contributors: {}",
                governance
                    .contributors
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            if let Some(cycle) = governance.cycles.last() {
                ui.label(
                    RichText::new(format!(
                        "Frozen approval route · policy v{} · submitted r{}",
                        cycle.policy.as_ref().map(|p| p.version).unwrap_or(0),
                        cycle.submitted_revision
                    ))
                    .color(NAVY)
                    .strong(),
                );
                for (index, stage) in cycle.stages.iter().enumerate() {
                    let progress = if let Some(review) = cycle.reviews.get(index) {
                        format!(
                            "{} by {} at r{}",
                            if review.approved {
                                "Approved"
                            } else {
                                "Rejected"
                            },
                            review.actor.name,
                            review.revision
                        )
                    } else if cycle.status() == Status::Rejected {
                        "Not reached".into()
                    } else if index == cycle.reviews.len() {
                        "Awaiting review".into()
                    } else {
                        "Waiting for previous stage".into()
                    };
                    ui.label(format!(
                        "{}. {:?}: {} — {}",
                        index + 1,
                        stage.role,
                        stage.reason,
                        progress
                    ));
                }
            }
        } else {
            ui.small("Legacy snapshot: the next submission captures a governed route.");
        }
    }
    fn governance(&mut self, ui: &mut egui::Ui) {
        ui.heading("Sequential quote review");
        ui.label("Every quote requires independent commercial approval. Enable a second Finance stage for price, margin or value exceptions. Each stage requires a different person; Admin cannot substitute for a required role.");
        ui.small("These are caller-selected local demo identities. This is not authentication or enterprise access control.");
        let current = self.engine.state().quote_approval_policies.last().cloned();
        if let Some(policy) = &current {
            ui.colored_label(
                TEAL,
                format!(
                    "Active policy v{} · saved r{}",
                    policy.version, policy.revision
                ),
            );
            ui.label(format!("Finance when effective invoice discount > {}%, weighted pocket margin < {}%, or pocket value ≥ USD {}, EUR {} or GBP {}.", policy.thresholds.max_discount, policy.thresholds.min_margin, policy.thresholds.usd_value, policy.thresholds.eur_value, policy.thresholds.gbp_value));
            if ui.button("Load current thresholds").clicked() {
                self.approval_discount = policy.thresholds.max_discount.to_string();
                self.approval_margin = policy.thresholds.min_margin.to_string();
                self.approval_usd = policy.thresholds.usd_value.to_string();
                self.approval_eur = policy.thresholds.eur_value.to_string();
                self.approval_gbp = policy.thresholds.gbp_value.to_string();
            }
        } else {
            ui.colored_label(COPPER, "Legacy default: one commercial review stage. Save thresholds to enable conditional Finance review.");
        }
        ui.separator();
        field(ui, "Discount above %", &mut self.approval_discount);
        field(ui, "Margin below %", &mut self.approval_margin);
        field(ui, "Value at least USD", &mut self.approval_usd);
        field(ui, "Value at least EUR", &mut self.approval_eur);
        field(ui, "Value at least GBP", &mut self.approval_gbp);
        if primary(ui, "Save approval policy") {
            let thresholds = (|| -> Result<ApprovalThresholds> {
                Ok(ApprovalThresholds {
                    max_discount: self.approval_discount.parse()?,
                    min_margin: self.approval_margin.parse()?,
                    usd_value: self.approval_usd.parse()?,
                    eur_value: self.approval_eur.parse()?,
                    gbp_value: self.approval_gbp.parse()?,
                })
            })();
            match thresholds {
                Ok(thresholds) => self.run(Command::QuoteApprovalPolicySet {
                    expected_version: current.as_ref().map(|p| p.version).unwrap_or(0),
                    thresholds,
                }),
                Err(e) => self.result::<()>(Err(e), ""),
            }
        }
        ui.small("Only Admin can save a policy. Pending and approved routes retain their frozen policy. Hard pricing guardrails cannot be overridden by review.");
        ui.separator();
        for policy in self.engine.state().quote_approval_policies.iter().rev() {
            ui.label(format!(
                "Policy v{} · audit r{} · discount {}% / margin {}%",
                policy.version,
                policy.revision,
                policy.thresholds.max_discount,
                policy.thresholds.min_margin
            ));
        }
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
    fn native_two_stage_quote_waits_for_finance()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let mut h = harness()?;
        h.state_mut().role = 3;
        h.state_mut().view = View::Governance;
        h.state_mut().approval_usd = "100.00".into();
        h.try_run().map_err(|e| e.to_string())?;
        h.get_by_label("Save approval policy").click();
        h.try_run().map_err(|e| e.to_string())?;
        assert_eq!(h.state().engine.state().quote_approval_policies.len(), 1);
        h.state_mut().role = 0;
        h.state_mut().view = View::Workbench;
        h.try_run().map_err(|e| e.to_string())?;
        h.get_by_label("Create quote").click();
        h.try_run().map_err(|e| e.to_string())?;
        h.get_by_label("Add selected product to quote").click();
        h.try_run().map_err(|e| e.to_string())?;
        let id = h.state().selected_quote.clone();
        h.get_by_label("Open quote").click();
        h.try_run().map_err(|e| e.to_string())?;
        h.get_by_label("Submit for approval").click();
        h.try_run().map_err(|e| e.to_string())?;
        h.state_mut().role = 2;
        h.state_mut().view = View::Approvals;
        h.try_run().map_err(|e| e.to_string())?;
        h.get_by_label("Approve quote").click();
        h.try_run().map_err(|e| e.to_string())?;
        assert_eq!(
            h.state().engine.state().quotes.get(&id).map(|q| q.status),
            Some(Status::Pending)
        );
        h.state_mut().role = 4;
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
            View::LiveGrid,
            View::Lists,
            View::Quotes,
            View::Approvals,
            View::Agreements,
            View::Analytics,
            View::Scenarios,
            View::Governance,
            View::Audit,
        ] {
            h.state_mut().view = view;
            h.try_run().map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

impl Workbench {
    fn live_grid(&mut self, ui: &mut egui::Ui) {
        self.context(ui);
        ui.label("Recalculate an item without replacing its last approved price. Rejecting a candidate preserves the active price.");
        ui.label("Grid candidates calculate from live rules. The workbench's approved-source selection applies only to quotes and scenarios.");
        ui.label(format!(
            "Selected product: {}. Change the product in Price workbench.",
            self.product
        ));
        if primary(ui, "Calculate grid candidate") {
            match self.request() {
                Ok(mut request) => {
                    request.source = None;
                    let id = format!("G-{}-{}", self.product, self.customer);
                    let version = self
                        .engine
                        .state()
                        .grid
                        .get(&id)
                        .map(|i| i.version)
                        .unwrap_or(0);
                    self.run(Command::GridCalculate {
                        id: id.clone(),
                        version,
                        request,
                    });
                    if !self.failed {
                        self.selected_grid = id;
                    }
                }
                Err(e) => self.result::<()>(Err(e), ""),
            }
        }
        ui.add_space(12.0);
        egui::Grid::new("live-grid-approval")
            .striped(true)
            .spacing([20.0, 12.0])
            .show(ui, |ui| {
                for h in [
                    "Item",
                    "Candidate / unit",
                    "Last approved / unit",
                    "Status",
                    "Source revision",
                ] {
                    ui.strong(h);
                }
                ui.end_row();
                for item in self.engine.state().grid.values() {
                    if ui
                        .selectable_label(self.selected_grid == item.id, &item.id)
                        .clicked()
                    {
                        self.selected_grid = item.id.clone();
                        self.product = item.candidate.request.product_id.clone();
                        self.customer = item.candidate.request.customer_id.clone();
                    }
                    ui.label(item.candidate.invoice_unit.to_string());
                    ui.label(
                        item.last_approved
                            .as_ref()
                            .map(|a| a.evaluation.invoice_unit.to_string())
                            .unwrap_or_else(|| "No approved price".into()),
                    );
                    ui.label(format!("{:?}", item.status));
                    ui.label(item.source_revision.to_string());
                    ui.end_row();
                }
            });
        if let Some(item) = self.engine.state().grid.get(&self.selected_grid).cloned() {
            ui.separator();
            ui.heading(format!("{} · Candidate v{}", item.id, item.version));
            Self::waterfall(ui, &item.candidate);
            field(ui, "Decision note", &mut self.note);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        item.status == Status::Draft,
                        egui::Button::new("Submit grid candidate"),
                    )
                    .clicked()
                {
                    self.run(Command::GridSubmit {
                        id: item.id.clone(),
                        version: item.version,
                    });
                }
                if ui
                    .add_enabled(
                        item.status == Status::Pending,
                        egui::Button::new("Approve and activate"),
                    )
                    .clicked()
                {
                    self.run(Command::GridDecide {
                        id: item.id.clone(),
                        version: item.version,
                        approve: true,
                        note: self.note.clone(),
                    });
                }
                if ui
                    .add_enabled(
                        item.status == Status::Pending,
                        egui::Button::new("Reject candidate"),
                    )
                    .clicked()
                {
                    self.run(Command::GridDecide {
                        id: item.id.clone(),
                        version: item.version,
                        approve: false,
                        note: self.note.clone(),
                    });
                }
            });
            if let Some(active) = &item.last_approved {
                ui.colored_label(
                    TEAL,
                    format!(
                        "Active approved price: {} {} / {} · {} · Approval r{}",
                        active.evaluation.currency,
                        active.evaluation.invoice_unit,
                        active.evaluation.unit,
                        active.decision.actor,
                        active.decision.revision
                    ),
                );
            }
        }
    }
}

impl Workbench {
    fn agreements(&mut self, ui: &mut egui::Ui) {
        self.context(ui);
        ui.label("Retrospective invoice rebates: the achieved tier applies to all eligible invoice revenue in the agreement period. No settlement or payment is performed.");
        ui.collapsing("Create a rebate agreement", |ui| {
            field(ui, "Agreement name", &mut self.title);
            field(ui, "Exclusive end date", &mut self.agreement_end);
            field(ui, "Base rebate %", &mut self.base_rate);
            field(ui, "Tier threshold · USD", &mut self.tier_threshold);
            field(ui, "Tier rebate %", &mut self.tier_rate);
            if primary(ui, "Create agreement") {
                let result = (|| {
                    Ok(AgreementTerms {
                        id: format!("A-{:04}", self.engine.state().revision + 1),
                        name: self.title.clone(),
                        customer_id: self.customer.clone(),
                        currency: Currency::USD,
                        start: Date::parse(&self.date)?,
                        end: Date::parse(&self.agreement_end)?,
                        tiers: vec![
                            RebateTier {
                                threshold: Money(0),
                                rate: self.base_rate.parse()?,
                            },
                            RebateTier {
                                threshold: self.tier_threshold.parse()?,
                                rate: self.tier_rate.parse()?,
                            },
                        ],
                    })
                })();
                match result {
                    Ok(terms) => {
                        let id = terms.id.clone();
                        self.run(Command::AgreementCreate { terms });
                        if !self.failed {
                            self.selected_agreement = id;
                        }
                    }
                    Err(e) => self.result::<()>(Err(e), ""),
                }
            }
        });
        ui.add_space(12.0);
        egui::Grid::new("agreements")
            .striped(true)
            .spacing([20.0, 12.0])
            .show(ui, |ui| {
                for h in ["Agreement", "Name", "Customer", "Period", "Status"] {
                    ui.strong(h);
                }
                ui.end_row();
                for a in self.engine.state().agreements.values() {
                    if ui
                        .selectable_label(self.selected_agreement == a.terms.id, &a.terms.id)
                        .clicked()
                    {
                        self.selected_agreement = a.terms.id.clone();
                    }
                    ui.label(&a.terms.name);
                    ui.label(&a.terms.customer_id);
                    ui.label(format!("{} to {}", a.terms.start, a.terms.end));
                    ui.label(format!("{:?}", a.status));
                    ui.end_row();
                }
            });
        if let Some(a) = self
            .engine
            .state()
            .agreements
            .get(&self.selected_agreement)
            .cloned()
        {
            ui.separator();
            ui.heading(format!(
                "{} · {:?} · v{}",
                a.terms.name, a.status, a.version
            ));
            for t in &a.terms.tiers {
                ui.label(format!(
                    "At {} {} invoice revenue: {}% rebate on all eligible revenue",
                    a.terms.currency, t.threshold, t.rate
                ));
            }
            field(ui, "Decision note", &mut self.note);
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(
                        a.status == Status::Draft,
                        egui::Button::new("Submit agreement"),
                    )
                    .clicked()
                {
                    self.run(Command::AgreementSubmit {
                        id: a.terms.id.clone(),
                        version: a.version,
                    });
                }
                if ui
                    .add_enabled(
                        a.status == Status::Pending,
                        egui::Button::new("Approve agreement"),
                    )
                    .clicked()
                {
                    self.run(Command::AgreementDecide {
                        id: a.terms.id.clone(),
                        version: a.version,
                        approve: true,
                        note: self.note.clone(),
                    });
                }
                if ui
                    .add_enabled(
                        a.status == Status::Pending,
                        egui::Button::new("Reject agreement"),
                    )
                    .clicked()
                {
                    self.run(Command::AgreementDecide {
                        id: a.terms.id.clone(),
                        version: a.version,
                        approve: false,
                        note: self.note.clone(),
                    });
                }
                if ui
                    .add_enabled(
                        a.status == Status::Approved,
                        egui::Button::new("Calculate accrual snapshot"),
                    )
                    .clicked()
                {
                    self.run(Command::RebateAccrue {
                        id: format!("R-{:04}", self.engine.state().revision + 1),
                        agreement_id: a.terms.id.clone(),
                        version: a.version,
                    });
                }
            });
        }
        ui.add_space(18.0);
        ui.heading("Accrual snapshots");
        ui.label("Each row recalculates the whole agreement period. Rows are alternatives at different data revisions; do not sum them.");
        egui::Grid::new("accruals")
            .striped(true)
            .spacing([20.0, 12.0])
            .show(ui, |ui| {
                for h in [
                    "Snapshot",
                    "Agreement",
                    "Source revision",
                    "Invoices",
                    "Eligible revenue",
                    "Rebate",
                ] {
                    ui.strong(h);
                }
                ui.end_row();
                for a in self.engine.state().accruals.values() {
                    ui.label(&a.id);
                    ui.label(&a.agreement_id);
                    ui.label(a.source_revision.to_string());
                    ui.label(a.transaction_ids.len().to_string());
                    ui.label(format!("{} {}", a.currency, a.eligible_invoice));
                    ui.label(a.rebate.to_string());
                    ui.end_row();
                }
            });
    }
    fn analytics_view(&mut self, ui: &mut egui::Ui) {
        ui.label("Recorded invoice data, separate from quote projections. Invoice margin excludes rebate accruals, tax and freight. Demo-generated invoices remain explicitly synthetic.");
        match copperline_engine::analytics(self.engine.state()) {
            Ok(groups) => {
                if groups.is_empty() {
                    ui.label("No recorded invoices. Add a synthetic invoice below or import transaction CSV through the command API.");
                }
                egui::Grid::new("invoice-analytics")
                    .striped(true)
                    .spacing([24.0, 14.0])
                    .show(ui, |ui| {
                        for h in [
                            "Customer",
                            "Currency",
                            "Invoices",
                            "Revenue",
                            "Cost",
                            "Invoice margin",
                            "Margin %",
                        ] {
                            ui.strong(h);
                        }
                        ui.end_row();
                        for g in groups {
                            ui.label(g.customer_id);
                            ui.label(g.currency.to_string());
                            ui.label(g.transaction_count.to_string());
                            ui.label(g.invoice.to_string());
                            ui.label(g.cost.to_string());
                            ui.label(g.invoice_margin.to_string());
                            ui.label(
                                g.margin_bps
                                    .map(|b| Bps(b).to_string())
                                    .unwrap_or_else(|| "Undefined".into()),
                            );
                            ui.end_row();
                        }
                    });
            }
            Err(e) => {
                ui.colored_label(Color32::DARK_RED, e.to_string());
            }
        }
        ui.separator();
        ui.heading("Record a synthetic invoice");
        self.context(ui);
        ui.label(format!("Use the current {} calculation from Price workbench. This is a local demo ledger, not an ERP posting.",self.product));
        if primary(ui, "Record synthetic invoice") {
            match self.live() {
                Ok(v) => self.run(Command::TransactionAdd(Transaction {
                    id: format!("T-{:04}", self.engine.state().revision + 1),
                    source: "synthetic-demo".into(),
                    product_id: v.request.product_id,
                    customer_id: v.request.customer_id,
                    date: v.request.date,
                    quantity: v.request.quantity,
                    unit: v.unit,
                    currency: v.currency,
                    invoice_total: v.invoice_total,
                    cost_total: v.cost_total,
                })),
                Err(e) => self.result::<()>(Err(e), ""),
            }
        }
        ui.add_space(14.0);
        egui::Grid::new("transactions")
            .striped(true)
            .spacing([20.0, 12.0])
            .show(ui, |ui| {
                for h in ["External ID", "Source", "Date", "Product", "Invoice"] {
                    ui.strong(h);
                }
                ui.end_row();
                for t in self.engine.state().transactions.values().rev().take(100) {
                    ui.label(&t.id);
                    ui.label(&t.source);
                    ui.label(t.date.to_string());
                    ui.label(&t.product_id);
                    ui.label(format!("{} {}", t.currency, t.invoice_total));
                    ui.end_row();
                }
            });
    }
}

#[cfg(test)]
mod grid_ui_tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};
    #[test]
    fn grid_buttons_activate_independent_candidate()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let mut engine = Engine::memory();
        engine.run(
            Actor {
                name: "Admin".into(),
                role: Role::Admin,
            },
            Command::SeedDemo,
        )?;
        let mut app = Workbench::new(engine);
        app.role = 1;
        app.view = View::LiveGrid;
        let mut h = Harness::builder()
            .with_size(egui::vec2(1440.0, 1400.0))
            .build_ui_state(|ui, app: &mut Workbench| app.ui(ui), app);
        h.get_by_label("Calculate grid candidate").click();
        h.try_run().map_err(|e| e.to_string())?;
        assert_eq!(h.state().engine.state().grid.len(), 1);
        h.get_by_label("Submit grid candidate").click();
        h.try_run().map_err(|e| e.to_string())?;
        h.state_mut().role = 2;
        h.try_run().map_err(|e| e.to_string())?;
        h.get_by_label("Approve and activate").click();
        h.try_run().map_err(|e| e.to_string())?;
        let item = h
            .state()
            .engine
            .state()
            .grid
            .values()
            .next()
            .ok_or_else(|| error("Grid item absent"))?;
        assert_eq!(item.status, Status::Approved);
        assert!(item.last_approved.is_some());
        Ok(())
    }
}
