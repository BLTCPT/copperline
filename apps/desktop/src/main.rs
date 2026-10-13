#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]
use copperline_engine::{Command, Engine};
use copperline_ui::{View, Workbench};
use eframe::egui;
struct Desktop {
    app: Workbench,
    screenshot: Option<std::path::PathBuf>,
    frames: u32,
}
impl eframe::App for Desktop {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.app.ui(ui);
        self.frames = self.frames.saturating_add(1);
        if self.frames == 3 && self.screenshot.is_some() {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
        if self.frames < 4 {
            ui.ctx().request_repaint();
        }
        let events = ui.input(|i| i.events.clone());
        for event in events {
            if let egui::Event::Screenshot { image, .. } = event
                && let Some(path) = self.screenshot.take()
            {
                let bytes: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
                if let Err(e) = image::save_buffer(
                    &path,
                    &bytes,
                    image.size[0] as u32,
                    image.size[1] as u32,
                    image::ColorType::Rgba8,
                ) {
                    eprintln!("Screenshot: {e}");
                }
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
}
fn main() {
    if let Err(e) = run() {
        eprintln!("Copperline desktop: {e}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let value = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    if args.iter().any(|a| a == "--version") {
        println!("Copperline {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    // A bad archive argument must fail before opening or bootstrapping any workspace.
    let inspected_backup = value("--backup-archive")
        .map(|path| Engine::inspect_backup(&path).map(|summary| (path, summary)))
        .transpose()?;
    let path = value("--data").unwrap_or_else(|| "data/demo".into());
    let mut engine = Engine::open(path)?;
    if args.iter().any(|a| a == "--demo") && engine.state().products.is_empty() {
        engine.run(
            copperline_model::Actor {
                name: "Demo administrator".into(),
                role: copperline_model::Role::Admin,
            },
            Command::SeedDemo,
        )?;
    }
    let mut app = Workbench::new(engine);
    if let Some((path, summary)) = inspected_backup {
        app.show_backup_inspection(&path, summary);
    }
    if let Some(id) = value("--source-grid") {
        let revision = app
            .engine
            .state()
            .grid
            .get(&id)
            .and_then(|item| item.last_approved.as_ref())
            .map(|price| price.decision.revision)
            .ok_or("Grid has no approved price")?;
        app.select_pricing_source(copperline_model::PriceSource::Grid {
            id,
            approval_revision: revision,
        })?;
    }
    if args.iter().any(|a| a == "--customer-contract") {
        app.select_pricing_source(copperline_model::PriceSource::CustomerContract)?;
    }
    if let Some(currency) = value("--currency") {
        app.select_conversion(&currency, &value("--unit").unwrap_or_else(|| "each".into()))?;
    }
    app.view = match value("--view").as_deref() {
        Some("catalog") => View::Catalog,
        Some("rules") => View::Rules,
        Some("grid") => View::LiveGrid,
        Some("contracts") => View::Contracts,
        Some("conversions") => View::Conversions,
        Some("lists") => View::Lists,
        Some("quotes") => View::Quotes,
        Some("bookings") => View::Bookings,
        Some("credits") => View::Credits,
        Some("period-close") => View::PeriodCloses,
        Some("approvals") => View::Approvals,
        Some("agreements") => View::Agreements,
        Some("analytics") => View::Analytics,
        Some("corrections") => View::Corrections,
        Some("settlements") => View::Settlements,
        Some("scenarios") => View::Scenarios,
        Some("policy") => View::Governance,
        Some("audit") => View::Audit,
        Some("backup") => View::Backup,
        _ => View::Workbench,
    };
    let screenshot = value("--screenshot").map(std::path::PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1440.0, 960.0])
            .with_min_inner_size([1050.0, 720.0]),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "Copperline — Pricing workbench",
        options,
        Box::new(move |_| {
            Ok(Box::new(Desktop {
                app,
                screenshot,
                frames: 0,
            }))
        }),
    )?;
    Ok(())
}
