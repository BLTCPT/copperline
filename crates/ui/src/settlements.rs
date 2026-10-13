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
        if agreements.is_empty() {
            ui.label("No approved rebate agreement yet. Create and independently approve an agreement before calculating a rebate adjustment.");
            if ui.button("Open agreements & rebates").clicked() {
                self.view = View::Agreements;
            }
            return;
        }
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
        let position = self
            .engine
            .state()
            .rebate_position(&self.selected_agreement, self.engine.state().revision);
        match &position {
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
                            ("Approved quantity credits", position.basis.credited_invoice),
                            ("Net eligible invoices", position.basis.eligible_invoice),
                            ("Full-period rebate target", position.target_rebate),
                            ("Approved local adjustments", position.recognized_rebate),
                        ] {
                            ui.label(label);
                            ui.label(format!("{} {}", position.currency, value));
                            ui.end_row();
                        }
                    });
                ui.label(format!("Retrospective tier {}% · {} invoice records · {} approved reversals · {} credit notes · source r{}",position.rate,position.basis.transaction_ids.len(),position.basis.correction_ids.len(),position.basis.credit_ids.len(),position.source_revision));
                ui.small("Credits restate the original invoice/agreement period at approval. Prior accrual snapshots remain alternatives; a new snapshot captures every approved credit ID, including zero-cent allocations.");
                if ui
                    .add_enabled(
                        matches!(self.actor().role, Role::Analyst | Role::Admin),
                        egui::Button::new("Calculate corrected accrual"),
                    )
                    .clicked()
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
        let adjustment_due = position
            .as_ref()
            .is_ok_and(|p| p.remaining_adjustment != Money(0));
        let can_create = matches!(self.actor().role, Role::Analyst | Role::Admin)
            && adjustment_due
            && self
                .engine
                .state()
                .accruals
                .contains_key(&self.settlement_form.accrual);
        if ui
            .add_enabled(can_create, egui::Button::new("Create settlement proposal"))
            .clicked()
        {
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
        if position
            .as_ref()
            .is_ok_and(|p| p.remaining_adjustment == Money(0))
        {
            ui.small("Current entitlement is fully recognized; no new adjustment is due.");
        } else if !matches!(self.actor().role, Role::Analyst | Role::Admin) {
            ui.small("Select Pricing or Admin to calculate an accrual and propose an adjustment.");
        }
        ui.small("Select a current accrual. Matching totals alone are insufficient: changed invoice, correction or credit provenance requires a new snapshot.");
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
            let current = self
                .engine
                .state()
                .ensure_settlement_current(record, self.engine.state().revision);
            let actor = self.actor();
            let owner =
                actor.name == record.owner && matches!(actor.role, Role::Analyst | Role::Admin);
            let reviewer =
                actor.role == Role::Finance && !record.contributors.contains(&actor.name);
            if matches!(record.status, RecordStatus::Draft | RecordStatus::Pending)
                && let Err(e) = &current
            {
                ui.colored_label(COPPER, e.to_string());
                ui.small("This proposal cannot be approved. Its owner can withdraw it; eligible Finance can reject a pending proposal. Calculate a new accrual only if an adjustment remains due.");
            } else if matches!(record.status, RecordStatus::Draft | RecordStatus::Pending) {
                ui.small("Only the owner submits or withdraws; independent Finance outside the source contributors approves or rejects.");
            }
            field(ui, "Decision note", &mut self.note);
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(
                        record.status == RecordStatus::Draft && owner && current.is_ok(),
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
                        record.status == RecordStatus::Pending && reviewer && current.is_ok(),
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
                        record.status == RecordStatus::Pending && reviewer,
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
                        matches!(record.status, RecordStatus::Draft | RecordStatus::Pending)
                            && owner,
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

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{
        Harness,
        kittest::{NodeT, Queryable},
    };
    type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

    fn actor(name: &str, role: Role) -> Actor {
        Actor {
            name: name.into(),
            role,
        }
    }
    fn render(h: &mut Harness<'_, Workbench>) -> TestResult {
        h.try_run().map_err(|e| e.to_string())?;
        Ok(())
    }
    fn seeded() -> Result<Engine> {
        let mut e = Engine::memory();
        e.run(actor("seed", Role::Admin), Command::SeedDemo)?;
        Ok(e)
    }
    fn raw(e: &mut Engine, id: &str) -> Result<()> {
        e.run(
            actor("invoice-author", Role::Analyst),
            Command::TransactionAdd(Transaction {
                id: id.into(),
                source: "synthetic-ui-review".into(),
                product_id: "VLV-100".into(),
                customer_id: "C-ATLAS".into(),
                date: Date::parse("2026-10-13")?,
                quantity: Quantity(1000),
                unit: "each".into(),
                currency: Currency::USD,
                invoice_total: Money(100),
                cost_total: Money(60),
            }),
        )?;
        Ok(())
    }
    fn snapshot_proposal(e: &mut Engine, accrual: &str, settlement: &str) -> Result<()> {
        let owner = actor("Morgan / Pricing", Role::Analyst);
        e.run(
            owner.clone(),
            Command::RebateAccrue {
                id: accrual.into(),
                agreement_id: "AG".into(),
                version: 3,
            },
        )?;
        e.run(
            owner.clone(),
            Command::SettlementCreate {
                id: settlement.into(),
                accrual_id: accrual.into(),
                reason: "Synthetic source snapshot".into(),
            },
        )?;
        e.run(
            owner,
            Command::SettlementSubmit {
                id: settlement.into(),
                version: 1,
            },
        )?;
        Ok(())
    }

    #[test]
    fn stale_settlement_disables_approval_keeps_rejection_and_zero_adjustment_guidance()
    -> TestResult {
        let mut e = seeded()?;
        e.run(
            actor("agreement-author", Role::Analyst),
            Command::AgreementCreate {
                terms: AgreementTerms {
                    id: "AG".into(),
                    name: "Synthetic review agreement".into(),
                    customer_id: "C-ATLAS".into(),
                    currency: Currency::USD,
                    start: Date::parse("2026-01-01")?,
                    end: Date::parse("2027-01-01")?,
                    tiers: vec![RebateTier {
                        threshold: Money(0),
                        rate: Bps(1000),
                    }],
                },
            },
        )?;
        e.run(
            actor("agreement-author", Role::Analyst),
            Command::AgreementSubmit {
                id: "AG".into(),
                version: 1,
            },
        )?;
        e.run(
            actor("Riley / Approval", Role::Approver),
            Command::AgreementDecide {
                id: "AG".into(),
                version: 2,
                approve: true,
                note: "Independent agreement review".into(),
            },
        )?;
        raw(&mut e, "FIRST")?;
        snapshot_proposal(&mut e, "BEFORE", "ST-2001")?;
        raw(&mut e, "SECOND")?;
        snapshot_proposal(&mut e, "AFTER", "ST-CURRENT")?;
        e.run(
            actor("Jordan / Finance", Role::Finance),
            Command::SettlementDecide {
                id: "ST-CURRENT".into(),
                version: 2,
                approve: true,
                note: "Current exact evidence".into(),
            },
        )?;
        let mut app = Workbench::new(e);
        app.view = View::Settlements;
        app.role = 1;
        app.settlement_form.selected = "ST-2001".into();
        let mut h = Harness::builder()
            .with_size(egui::vec2(1600.0, 1800.0))
            .build_ui_state(|ui, app: &mut Workbench| app.ui(ui), app);
        assert!(
            h.get_by_label("Approve settlement")
                .accesskit_node()
                .is_disabled()
        );
        assert!(
            h.get_by_label("Reject settlement")
                .accesskit_node()
                .is_disabled()
        );
        assert!(
            !h.get_by_label("Withdraw settlement")
                .accesskit_node()
                .is_disabled()
        );
        assert!(
            h.get_by_label("Create settlement proposal")
                .accesskit_node()
                .is_disabled()
        );
        assert!(
            h.query_by_label("Current entitlement is fully recognized; no new adjustment is due.")
                .is_some()
        );
        assert!(h.query_by_label("This proposal cannot be approved. Its owner can withdraw it; eligible Finance can reject a pending proposal. Calculate a new accrual only if an adjustment remains due.").is_some());
        h.state_mut().role = 3;
        render(&mut h)?;
        assert!(
            h.get_by_label("Withdraw settlement")
                .accesskit_node()
                .is_disabled()
        );
        h.state_mut().role = 4;
        render(&mut h)?;
        assert!(
            h.get_by_label("Approve settlement")
                .accesskit_node()
                .is_disabled()
        );
        assert!(
            !h.get_by_label("Reject settlement")
                .accesskit_node()
                .is_disabled()
        );
        let before = h.state().engine.state().clone();
        h.get_by_label("Approve settlement").click();
        render(&mut h)?;
        assert_eq!(h.state().engine.state(), &before);
        assert!(!h.state().failed);
        h.get_by_label("Reject settlement").click();
        render(&mut h)?;
        assert_eq!(
            h.state()
                .engine
                .state()
                .settlements
                .get("ST-2001")
                .map(|s| s.status),
            Some(RecordStatus::Rejected)
        );
        assert!(!h.state().failed, "{}", h.state().status);
        Ok(())
    }

    #[test]
    fn empty_reconciliation_offers_approved_agreement_next_step() -> TestResult {
        let mut app = Workbench::new(seeded()?);
        app.view = View::Settlements;
        let mut h = Harness::builder()
            .with_size(egui::vec2(1200.0, 900.0))
            .build_ui_state(|ui, app: &mut Workbench| app.ui(ui), app);
        assert!(h.query_by_label("Create settlement proposal").is_none());
        assert!(h.query_by_label("Agreement not found").is_none());
        h.get_by_label("Open agreements & rebates").click();
        render(&mut h)?;
        assert!(h.state().view == View::Agreements);
        assert!(!h.state().failed);
        Ok(())
    }
}
