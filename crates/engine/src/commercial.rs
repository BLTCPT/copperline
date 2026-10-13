use super::*;
use std::collections::BTreeMap;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevenueGroup {
    pub customer_id: String,
    pub currency: Currency,
    pub transaction_count: usize,
    pub gross_transaction_count: usize,
    pub reversal_count: usize,
    pub gross_invoice: Money,
    pub gross_cost: Money,
    pub reversed_invoice: Money,
    pub reversed_cost: Money,
    pub invoice: Money,
    pub cost: Money,
    pub invoice_margin: Money,
    pub margin_bps: Option<i32>,
}
/// Recorded invoice actuals only. Quote projections and rebate accrual snapshots are never added.
pub fn analytics(state: &State) -> Result<Vec<RevenueGroup>> {
    let mut groups: BTreeMap<(String, String), RevenueGroup> = BTreeMap::new();
    for t in state.transactions.values() {
        let g = groups
            .entry((t.customer_id.clone(), t.currency.to_string()))
            .or_insert(RevenueGroup {
                customer_id: t.customer_id.clone(),
                currency: t.currency,
                transaction_count: 0,
                gross_transaction_count: 0,
                reversal_count: 0,
                gross_invoice: Money(0),
                gross_cost: Money(0),
                reversed_invoice: Money(0),
                reversed_cost: Money(0),
                invoice: Money(0),
                cost: Money(0),
                invoice_margin: Money(0),
                margin_bps: None,
            });
        g.gross_transaction_count = g
            .gross_transaction_count
            .checked_add(1)
            .ok_or_else(|| error("Invoice count overflow"))?;
        g.gross_invoice = g.gross_invoice.add(t.invoice_total)?;
        g.gross_cost = g.gross_cost.add(t.cost_total)?;
    }
    let mut reversed = std::collections::BTreeSet::new();
    for c in state
        .corrections
        .values()
        .filter(|c| c.status == RecordStatus::Approved)
    {
        ensure(
            reversed.insert(&c.proposal.original_transaction_id),
            "Invoice cannot be reversed twice",
        )?;
        let t = state
            .transactions
            .get(&c.proposal.original_transaction_id)
            .ok_or_else(|| error("Reversed invoice missing"))?;
        let g = groups
            .get_mut(&(t.customer_id.clone(), t.currency.to_string()))
            .ok_or_else(|| error("Reversal analytics group missing"))?;
        g.reversal_count = g
            .reversal_count
            .checked_add(1)
            .ok_or_else(|| error("Reversal count overflow"))?;
        g.reversed_invoice = g.reversed_invoice.add(t.invoice_total)?;
        g.reversed_cost = g.reversed_cost.add(t.cost_total)?;
    }
    for g in groups.values_mut() {
        g.transaction_count = g
            .gross_transaction_count
            .checked_sub(g.reversal_count)
            .ok_or_else(|| error("Reversal count exceeds invoice count"))?;
        g.invoice = g.gross_invoice.sub(g.reversed_invoice)?;
        g.cost = g.gross_cost.sub(g.reversed_cost)?;
        ensure(
            g.invoice.0 >= 0 && g.cost.0 >= 0,
            "Reversal exceeds gross invoices",
        )?;
        g.invoice_margin = g.invoice.sub(g.cost)?;
        if g.invoice.0 > 0 {
            g.margin_bps = Some(
                i32::try_from(copperline_money::ratio(
                    g.invoice_margin.0,
                    10_000,
                    g.invoice.0,
                )?)
                .map_err(|_| error("Actuals margin overflow"))?,
            );
        }
    }
    Ok(groups.into_values().collect())
}
pub(super) fn apply(s: &mut State, actor: &Actor, c: &Command) -> Result<String> {
    match c {
        Command::AgreementCreate { terms } => {
            terms.validate()?;
            ensure(
                !s.agreements.contains_key(&terms.id),
                "Agreement ID exists; approved terms are immutable",
            )?;
            s.agreements.insert(
                terms.id.clone(),
                Agreement {
                    terms: terms.clone(),
                    owner: actor.name.clone(),
                    version: 1,
                    source_revision: s.revision,
                    status: Status::Draft,
                    submitted_by: None,
                    decisions: vec![],
                },
            );
            Ok(format!("Created rebate agreement {}", terms.id))
        }
        Command::AgreementSubmit { id, version } => {
            let a = s
                .agreements
                .get_mut(id)
                .ok_or_else(|| error("Agreement not found"))?;
            ensure(
                a.version == *version && a.status == Status::Draft,
                "Submit current draft agreement",
            )?;
            a.status = Status::Pending;
            a.submitted_by = Some(actor.name.clone());
            a.version = next_version(a.version)?;
            Ok(format!("Submitted agreement {id}"))
        }
        Command::AgreementDecide {
            id,
            version,
            approve,
            note,
        } => {
            text(note)?;
            let a = s
                .agreements
                .get_mut(id)
                .ok_or_else(|| error("Agreement not found"))?;
            ensure(
                a.version == *version && a.status == Status::Pending,
                "Decide current pending agreement",
            )?;
            ensure(
                a.owner != actor.name && a.submitted_by.as_ref() != Some(&actor.name),
                "A different person must approve the agreement",
            )?;
            a.status = if *approve {
                Status::Approved
            } else {
                Status::Rejected
            };
            a.decisions.push(Decision {
                actor: actor.name.clone(),
                note: note.clone(),
                revision: s.revision + 1,
                approved: *approve,
            });
            a.version = next_version(a.version)?;
            Ok(format!(
                "{} agreement {id}",
                if *approve { "Approved" } else { "Rejected" }
            ))
        }
        Command::TransactionAdd(t) => {
            s.validate_transaction(t)?;
            ensure(
                s.transaction_id_available(&t.id),
                "External transaction ID already exists or is reserved by a correction",
            )?;
            s.transaction_revisions.insert(t.id.clone(), s.revision + 1);
            s.transactions.insert(t.id.clone(), t.clone());
            analytics(s)?;
            Ok(format!("Recorded invoice {}", t.id))
        }
        Command::TransactionsImport { csv } => {
            let rows = super::csv::rows(csv)?;
            ensure(
                rows.first().is_some_and(|r| {
                    r == &[
                        "id",
                        "source",
                        "product_id",
                        "customer_id",
                        "date",
                        "quantity",
                        "unit",
                        "currency",
                        "invoice_total",
                        "cost_total",
                    ]
                }),
                "Expected transaction CSV header: id,source,product_id,customer_id,date,quantity,unit,currency,invoice_total,cost_total",
            )?;
            ensure(rows.len() > 1, "CSV has no transactions")?;
            for row in rows.iter().skip(1) {
                ensure(row.len() == 10, "Transaction CSV row requires 10 columns")?;
                let f = |i| {
                    row.get(i)
                        .cloned()
                        .ok_or_else(|| error("Missing transaction field"))
                };
                let t = Transaction {
                    id: f(0)?,
                    source: f(1)?,
                    product_id: f(2)?,
                    customer_id: f(3)?,
                    date: Date::parse(&f(4)?)?,
                    quantity: f(5)?.parse()?,
                    unit: f(6)?,
                    currency: match f(7)?.as_str() {
                        "USD" => Currency::USD,
                        "EUR" => Currency::EUR,
                        "GBP" => Currency::GBP,
                        _ => return Err(error("Unsupported transaction currency")),
                    },
                    invoice_total: f(8)?.parse()?,
                    cost_total: f(9)?.parse()?,
                };
                ensure(
                    s.transaction_id_available(&t.id),
                    "Duplicate or reserved external transaction ID; no rows imported",
                )?;
                s.validate_transaction(&t)?;
                s.transaction_revisions.insert(t.id.clone(), s.revision + 1);
                s.transactions.insert(t.id.clone(), t);
            }
            analytics(s)?;
            Ok(format!("Imported {} invoices atomically", rows.len() - 1))
        }
        Command::RebateAccrue {
            id: key,
            agreement_id,
            version,
        } => {
            id(key)?;
            ensure(!s.accruals.contains_key(key), "Accrual ID already exists")?;
            let a = s
                .agreements
                .get(agreement_id)
                .ok_or_else(|| error("Agreement not found"))?;
            ensure(
                a.version == *version && a.status == Status::Approved,
                "Accrual requires current approved agreement version",
            )?;
            let basis = s.accrual_basis(agreement_id, s.revision)?;
            let invoice = basis.eligible_invoice;
            ensure(
                !basis.transaction_ids.is_empty(),
                "No eligible recorded invoices in agreement period",
            )?;
            let rate = a
                .terms
                .tiers
                .iter()
                .filter(|t| t.threshold <= invoice)
                .map(|t| t.rate)
                .next_back()
                .ok_or_else(|| error("No applicable tier"))?;
            let rebate = invoice.percent(rate)?;
            s.accruals.insert(
                key.clone(),
                Accrual {
                    id: key.clone(),
                    agreement_id: agreement_id.clone(),
                    agreement_version: *version,
                    source_revision: s.revision,
                    currency: a.terms.currency,
                    transaction_ids: basis.transaction_ids,
                    correction_ids: basis.correction_ids,
                    eligible_invoice: invoice,
                    rate,
                    rebate,
                },
            );
            Ok(format!(
                "Calculated accrual {key}: {rebate}; no payment or posting"
            ))
        }
        _ => Err(error("Not a commercial command")),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn a(name: &str, role: Role) -> Actor {
        Actor {
            name: name.into(),
            role,
        }
    }
    #[test]
    fn retrospective_tier_accrual_is_frozen_and_approved() -> Result<()> {
        let mut e = Engine::memory();
        e.run(a("admin", Role::Admin), Command::SeedDemo)?;
        let terms = AgreementTerms {
            id: "A1".into(),
            name: "Synthetic annual rebate".into(),
            customer_id: "C-ATLAS".into(),
            currency: Currency::USD,
            start: Date::parse("2026-01-01")?,
            end: Date::parse("2027-01-01")?,
            tiers: vec![
                RebateTier {
                    threshold: Money(0),
                    rate: Bps(200),
                },
                RebateTier {
                    threshold: Money(100000),
                    rate: Bps(500),
                },
            ],
        };
        e.run(
            a("pricing", Role::Analyst),
            Command::AgreementCreate {
                terms: terms.clone(),
            },
        )?;
        assert!(
            e.run(
                a("pricing", Role::Analyst),
                Command::RebateAccrue {
                    id: "R0".into(),
                    agreement_id: "A1".into(),
                    version: 1
                }
            )
            .is_err()
        );
        e.run(
            a("pricing", Role::Analyst),
            Command::AgreementSubmit {
                id: "A1".into(),
                version: 1,
            },
        )?;
        e.run(
            a("reviewer", Role::Approver),
            Command::AgreementDecide {
                id: "A1".into(),
                version: 2,
                approve: true,
                note: "Reviewed".into(),
            },
        )?;
        let t = Transaction {
            id: "T1".into(),
            source: "synthetic-test".into(),
            product_id: "VLV-100".into(),
            customer_id: "C-ATLAS".into(),
            date: Date::parse("2026-10-13")?,
            quantity: Quantity(10000),
            unit: "each".into(),
            currency: Currency::USD,
            invoice_total: Money(90000),
            cost_total: Money(60000),
        };
        e.run(
            a("pricing", Role::Analyst),
            Command::TransactionAdd(t.clone()),
        )?;
        e.run(
            a("pricing", Role::Analyst),
            Command::RebateAccrue {
                id: "R1".into(),
                agreement_id: "A1".into(),
                version: 3,
            },
        )?;
        let first = e
            .state()
            .accruals
            .get("R1")
            .cloned()
            .ok_or_else(|| error("No accrual"))?;
        assert_eq!(first.rebate, Money(1800));
        let mut second = t;
        second.id = "T2".into();
        second.invoice_total = Money(10000);
        second.cost_total = Money(5000);
        e.run(
            a("pricing", Role::Analyst),
            Command::TransactionAdd(second.clone()),
        )?;
        e.run(
            a("pricing", Role::Analyst),
            Command::RebateAccrue {
                id: "R2".into(),
                agreement_id: "A1".into(),
                version: 3,
            },
        )?;
        assert_eq!(
            e.state().accruals.get("R2").map(|a| a.rebate),
            Some(Money(5000))
        );
        assert_eq!(e.state().accruals.get("R1"), Some(&first));
        assert!(
            e.run(a("pricing", Role::Analyst), Command::TransactionAdd(second))
                .is_err()
        );
        let groups = analytics(e.state())?;
        assert_eq!(groups.first().map(|g| g.margin_bps), Some(Some(3500)));
        let mut overlapping = terms;
        overlapping.id = "A2".into();
        e.run(
            a("pricing", Role::Analyst),
            Command::AgreementCreate { terms: overlapping },
        )?;
        e.run(
            a("pricing", Role::Analyst),
            Command::AgreementSubmit {
                id: "A2".into(),
                version: 1,
            },
        )?;
        let before = e.state().clone();
        assert!(
            e.run(
                a("reviewer", Role::Approver),
                Command::AgreementDecide {
                    id: "A2".into(),
                    version: 2,
                    approve: true,
                    note: "Must reject overlap".into()
                }
            )
            .is_err()
        );
        assert_eq!(e.state(), &before);
        Ok(())
    }
}
