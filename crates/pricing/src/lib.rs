//! Pure, deterministic pricing. No IO, clocks, UI, randomness or business writes.
use copperline_model::*;
use copperline_money::{Bps, Money, ratio};
use std::collections::BTreeMap;

pub fn evaluate(state: &State, request: &PriceRequest) -> Result<Evaluation> {
    request.date.validate()?;
    request.quantity.validate()?;
    request.discount.validate()?;
    request.rebate.validate()?;
    let product = state
        .products
        .get(&request.product_id)
        .ok_or_else(|| error("Product not found"))?;
    let customer = state
        .customers
        .get(&request.customer_id)
        .ok_or_else(|| error("Customer not found"))?;
    let cost = state
        .costs
        .iter()
        .filter(|c| c.product_id == product.id && c.effective_from <= request.date)
        .max_by_key(|c| (&c.effective_from, c.version))
        .ok_or_else(|| error("No effective cost; add a cost input for this date"))?;
    ensure(
        cost.currency == product.currency && cost.unit == product.unit,
        "Cost currency/unit mismatch",
    )?;
    // Versions supersede the same rule ID only after their own effective start.
    let mut latest: BTreeMap<&str, &Rule> = BTreeMap::new();
    for r in state
        .rules
        .iter()
        .filter(|r| r.effective_from <= request.date)
    {
        if latest
            .get(r.id.as_str())
            .is_none_or(|old| r.version > old.version)
        {
            latest.insert(&r.id, r);
        }
    }
    let rule = latest
        .values()
        .copied()
        .filter(|r| {
            r.currency == product.currency
                && r.effective_to
                    .as_ref()
                    .is_none_or(|end| &request.date < end)
                && r.min_quantity <= request.quantity
                && r.product_id.as_ref().is_none_or(|id| id == &product.id)
                && r.customer_id.as_ref().is_none_or(|id| id == &customer.id)
                && r.segment.as_ref().is_none_or(|s| s == &customer.segment)
        })
        .max_by_key(|r| {
            (
                r.priority,
                r.specificity(),
                &r.effective_from,
                r.version,
                &r.id,
            )
        });
    let list = product.list_price;
    let ruled = match rule.map(|r| &r.action) {
        Some(RuleAction::Discount(b)) => list.sub(list.percent(*b)?)?,
        Some(RuleAction::Markup(b)) => cost.amount.add(cost.amount.percent(*b)?)?,
        Some(RuleAction::Fixed(m)) => *m,
        None => list,
    };
    let invoice = ruled.sub(ruled.percent(request.discount)?)?;
    let rebate = invoice.percent(request.rebate)?;
    let pocket = invoice.sub(rebate)?;
    let invoice_total = invoice.extend(request.quantity)?;
    // Apply rebate to extended invoice, avoiding multiplication of already rounded unit rebates.
    let rebate_total = invoice_total.percent(request.rebate)?;
    let pocket_total = invoice_total.sub(rebate_total)?;
    let cost_total = cost.amount.extend(request.quantity)?;
    let margin_total = pocket_total.sub(cost_total)?;
    let margin_bps = if pocket_total.0 > 0 {
        Some(
            i32::try_from(ratio(margin_total.0, 10_000, pocket_total.0)?)
                .map_err(|_| error("Margin ratio outside supported range"))?,
        )
    } else {
        None
    };
    let below = |floor: Bps| {
        i128::from(margin_total.0) * 10_000 < i128::from(pocket_total.0) * i128::from(floor.0)
    };
    let mut warnings = vec![];
    if pocket_total.0 == 0 {
        warnings.push("Zero pocket revenue: margin is undefined".into());
    }
    if below(state.policy.min_margin) {
        warnings.push(format!(
            "Pocket margin below {}% approval floor",
            state.policy.min_margin
        ));
    }
    let discount_minor = list.sub(invoice)?;
    if i128::from(discount_minor.0) * 10_000
        > i128::from(list.0) * i128::from(state.policy.max_discount.0)
    {
        warnings.push(format!(
            "Effective invoice discount exceeds {}%",
            state.policy.max_discount
        ));
    }
    let hard_block = pocket_total.0 == 0 || below(state.policy.hard_min_margin);
    if hard_block {
        warnings.push("Hard margin floor: revise this line before submission".into());
    }
    let rule_label = rule.map(|r| format!("{} v{} · {}", r.id, r.version, r.name));
    Ok(Evaluation {
        request: request.clone(),
        currency: product.currency,
        unit: product.unit.clone(),
        product_name: product.name.clone(),
        list_unit: list,
        rule_unit: ruled,
        invoice_unit: invoice,
        rebate_unit: rebate,
        pocket_unit: pocket,
        cost_unit: cost.amount,
        invoice_total,
        rebate_total,
        pocket_total,
        cost_total,
        margin_total,
        margin_bps,
        rule: rule_label,
        cost_version: cost.version,
        steps: vec![
            Step {
                label: "Catalog list / unit".into(),
                amount: list,
            },
            Step {
                label: "Selected rule / unit".into(),
                amount: ruled,
            },
            Step {
                label: "Invoice / unit".into(),
                amount: invoice,
            },
            Step {
                label: "Rebate / unit (indicative)".into(),
                amount: Money(-rebate.0),
            },
            Step {
                label: "Pocket / unit (indicative)".into(),
                amount: pocket,
            },
            Step {
                label: "Cost / unit".into(),
                amount: cost.amount,
            },
        ],
        warnings,
        hard_block,
    })
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Totals {
    pub currency: Currency,
    pub invoice: Money,
    pub rebate: Money,
    pub pocket: Money,
    pub cost: Money,
    pub margin: Money,
    pub margin_bps: Option<i32>,
}
pub fn totals(lines: &[Evaluation]) -> Result<Totals> {
    let first = lines
        .first()
        .ok_or_else(|| error("Add at least one line"))?;
    let mut t = Totals {
        currency: first.currency,
        invoice: Money(0),
        rebate: Money(0),
        pocket: Money(0),
        cost: Money(0),
        margin: Money(0),
        margin_bps: None,
    };
    for l in lines {
        ensure(
            l.currency == t.currency,
            "Mixed currencies require an FX adapter; separate the document by currency",
        )?;
        t.invoice = t.invoice.add(l.invoice_total)?;
        t.rebate = t.rebate.add(l.rebate_total)?;
        t.pocket = t.pocket.add(l.pocket_total)?;
        t.cost = t.cost.add(l.cost_total)?;
    }
    t.margin = t.pocket.sub(t.cost)?;
    if t.pocket.0 > 0 {
        t.margin_bps = Some(
            i32::try_from(ratio(t.margin.0, 10_000, t.pocket.0)?)
                .map_err(|_| error("Margin overflow"))?,
        );
    }
    Ok(t)
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Comparison {
    pub baseline: Evaluation,
    pub scenario: Evaluation,
    pub pocket_delta: Money,
    pub margin_delta: Money,
}
pub fn compare(
    state: &State,
    baseline: &PriceRequest,
    scenario: &PriceRequest,
) -> Result<Comparison> {
    ensure(
        baseline.product_id == scenario.product_id && baseline.customer_id == scenario.customer_id,
        "Compare the same product and customer",
    )?;
    let baseline = evaluate(state, baseline)?;
    let scenario = evaluate(state, scenario)?;
    Ok(Comparison {
        pocket_delta: scenario.pocket_total.sub(baseline.pocket_total)?,
        margin_delta: scenario.margin_total.sub(baseline.margin_total)?,
        baseline,
        scenario,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use copperline_money::Quantity;
    fn fixture() -> Result<(State, PriceRequest)> {
        let mut s = State::default();
        s.products.insert(
            "P".into(),
            Product {
                id: "P".into(),
                name: "Synthetic valve".into(),
                family: "Flow".into(),
                unit: "each".into(),
                currency: Currency::USD,
                list_price: Money(10000),
            },
        );
        s.customers.insert(
            "C".into(),
            Customer {
                id: "C".into(),
                name: "Synthetic buyer".into(),
                segment: "OEM".into(),
            },
        );
        s.costs.push(Cost {
            product_id: "P".into(),
            version: 1,
            effective_from: Date::parse("2026-01-01")?,
            amount: Money(6000),
            currency: Currency::USD,
            unit: "each".into(),
        });
        Ok((
            s,
            PriceRequest {
                product_id: "P".into(),
                customer_id: "C".into(),
                date: Date::parse("2026-10-13")?,
                quantity: Quantity(10000),
                discount: Bps(1000),
                rebate: Bps(500),
            },
        ))
    }
    #[test]
    fn golden_waterfall() -> Result<()> {
        let (s, r) = fixture()?;
        let p = evaluate(&s, &r)?;
        assert_eq!(
            (
                p.invoice_total,
                p.rebate_total,
                p.pocket_total,
                p.margin_total
            ),
            (Money(90000), Money(4500), Money(85500), Money(25500))
        );
        assert_eq!(p.margin_bps, Some(2982));
        Ok(())
    }
    #[test]
    fn missing_cost_and_zero_revenue() -> Result<()> {
        let (mut s, mut r) = fixture()?;
        r.discount = Bps(10000);
        let p = evaluate(&s, &r)?;
        assert_eq!(p.margin_bps, None);
        assert!(p.hard_block);
        s.costs.clear();
        assert!(evaluate(&s, &r).is_err());
        Ok(())
    }
    #[test]
    fn rule_precedence_and_effectivity() -> Result<()> {
        let (mut s, mut r) = fixture()?;
        r.discount = Bps(0);
        r.rebate = Bps(0);
        let rule = Rule {
            id: "A".into(),
            version: 1,
            name: "OEM price".into(),
            priority: 10,
            product_id: None,
            customer_id: None,
            segment: Some("OEM".into()),
            effective_from: Date::parse("2026-01-01")?,
            effective_to: None,
            min_quantity: Quantity(1000),
            action: RuleAction::Discount(Bps(1000)),
            currency: Currency::USD,
        };
        s.rules.push(rule.clone());
        let mut higher = rule.clone();
        higher.id = "B".into();
        higher.customer_id = Some("C".into());
        higher.action = RuleAction::Fixed(Money(8000));
        s.rules.push(higher);
        assert_eq!(evaluate(&s, &r)?.rule_unit, Money(8000));
        let mut future = rule;
        future.version = 2;
        future.priority = 99;
        future.effective_from = Date::parse("2027-01-01")?;
        future.action = RuleAction::Fixed(Money(7500));
        s.rules.push(future);
        assert_eq!(evaluate(&s, &r)?.rule_unit, Money(8000));
        r.date = Date::parse("2027-01-01")?;
        assert_eq!(evaluate(&s, &r)?.rule_unit, Money(7500));
        Ok(())
    }
    #[test]
    fn conservation_over_quantities_discounts() -> Result<()> {
        let (s, mut r) = fixture()?;
        for q in [1, 99, 1000, 1999, 10000, 1234567] {
            r.quantity = Quantity(q);
            for d in (0..=10000).step_by(137) {
                r.discount = Bps(d);
                let p = evaluate(&s, &r)?;
                assert_eq!(p.invoice_total.sub(p.rebate_total)?, p.pocket_total);
                assert_eq!(p.pocket_total.sub(p.cost_total)?, p.margin_total);
            }
        }
        Ok(())
    }
}
