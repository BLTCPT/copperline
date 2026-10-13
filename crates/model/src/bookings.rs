//! Full approved-quote booking. Native amounts are non-posted pricing evidence.
use super::*;
use copperline_money::Rational;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BookingRequest {
    pub id: String,
    pub quote_id: String,
    pub quote_version: u64,
    pub invoice_date: Date,
    pub source: String,
}
impl BookingRequest {
    pub fn validate(&self) -> Result<()> {
        id(&self.id)?;
        ensure(
            self.id.len() <= 59,
            "Booking ID is limited to 59 characters",
        )?;
        id(&self.quote_id)?;
        ensure(
            self.quote_version > 0,
            "Booking requires an approved quote version",
        )?;
        self.invoice_date.validate()?;
        text(&self.source)
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeBookingBasis {
    pub currency: Currency,
    pub unit: String,
    pub quantity: Quantity,
    pub invoice_unit: Money,
    pub cost_unit: Money,
    pub invoice_total: Money,
    pub cost_total: Money,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BookingRoundingDifference {
    /// Booked minus forward-FX-converted non-posted native pricing basis.
    pub invoice: Money,
    pub cost: Money,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BookedQuoteLine {
    pub index: usize,
    pub booked: Transaction,
    pub native_basis: NativeBookingBasis,
    pub reconciliation: BookingRoundingDifference,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BookingRounding {
    ExactApprovedTotalsNativeUnitDiscountThenExtendV1,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuoteBooking {
    pub request: BookingRequest,
    pub owner: String,
    pub created_revision: u64,
    pub rounding: BookingRounding,
    pub approval_cycle: QuoteApprovalCycle,
    pub quote_contributors: BTreeSet<String>,
    pub lines: Vec<BookedQuoteLine>,
}
pub fn booking_target(id: &str) -> String {
    format!("booking:{id}")
}
pub fn booking_created_detail(booking: &QuoteBooking) -> Result<String> {
    Ok(format!(
        "Booked quote {}: {}",
        booking.request.id,
        record_fingerprint(booking)?
    ))
}
fn derived_line(
    request: &BookingRequest,
    line: &Evaluation,
    index: usize,
) -> Result<BookedQuoteLine> {
    line.validate_snapshot()?;
    let (currency, unit, quantity, rule, cost, fx) = if let Some(c) = &line.conversion {
        (
            c.native_currency,
            c.native_unit.clone(),
            c.native_quantity,
            c.native_rule_unit,
            c.native_cost_unit,
            c.fx.as_ref()
                .map(|r| r.entry.ratio)
                .unwrap_or(Rational::ONE),
        )
    } else {
        (
            line.currency,
            line.unit.clone(),
            line.request.quantity,
            line.rule_unit,
            line.cost_unit,
            Rational::ONE,
        )
    };
    let invoice_unit = rule.sub(rule.percent(line.request.discount)?)?;
    let native_basis = NativeBookingBasis {
        currency,
        unit,
        quantity,
        invoice_unit,
        cost_unit: cost,
        invoice_total: invoice_unit.extend(quantity)?,
        cost_total: cost.extend(quantity)?,
    };
    let number = index
        .checked_add(1)
        .ok_or_else(|| error("Booking line overflow"))?;
    let booked = Transaction {
        id: format!("{}-L{number:03}", request.id),
        source: request.source.clone(),
        product_id: line.request.product_id.clone(),
        customer_id: line.request.customer_id.clone(),
        date: request.invoice_date.clone(),
        quantity: line.request.quantity,
        unit: line.unit.clone(),
        currency: line.currency,
        invoice_total: line.invoice_total,
        cost_total: line.cost_total,
    };
    let reconciliation = BookingRoundingDifference {
        invoice: booked
            .invoice_total
            .sub(fx.money(native_basis.invoice_total)?)?,
        cost: booked.cost_total.sub(fx.money(native_basis.cost_total)?)?,
    };
    Ok(BookedQuoteLine {
        index,
        booked,
        native_basis,
        reconciliation,
    })
}
impl State {
    /// Pure, exact preview; no price/rate/contract is resolved at current terms.
    pub fn preview_quote_booking(&self, request: &BookingRequest) -> Result<Vec<BookedQuoteLine>> {
        request.validate()?;
        ensure(
            !self.quote_bookings.contains_key(&request.id),
            "Booking ID already exists",
        )?;
        ensure(
            !self
                .quote_bookings
                .values()
                .any(|b| b.request.quote_id == request.quote_id),
            "This quote was already fully booked; reversal does not release it",
        )?;
        let quote = self
            .quotes
            .get(&request.quote_id)
            .ok_or_else(|| error("Quote not found"))?;
        ensure(
            quote.status == Status::Approved && quote.version == request.quote_version,
            "Book the current approved quote version",
        )?;
        ensure(
            request.invoice_date >= quote.date,
            "Invoice date precedes approved pricing date",
        )?;
        let cycle = quote
            .governance
            .as_ref()
            .and_then(|g| g.cycles.last())
            .ok_or_else(|| {
                error("Legacy quote must be revised and independently reapproved before booking")
            })?;
        ensure(
            cycle.status() == Status::Approved && cycle.lines == quote.lines,
            "Booking requires the complete frozen approved cycle",
        )?;
        let lines: Vec<_> = cycle
            .lines
            .iter()
            .enumerate()
            .map(|(index, line)| derived_line(request, line, index))
            .collect::<Result<_>>()?;
        for line in &lines {
            self.ensure_invoice_open_at(&line.booked, self.revision)?;
            ensure(
                self.transaction_id_available(&line.booked.id),
                "Booking invoice ID exists or is reserved by a correction",
            )?;
        }
        Ok(lines)
    }
    /// Indexed by the reserved deterministic suffix, with exact membership checked.
    pub fn booking_for_transaction(&self, id: &str) -> Option<(&QuoteBooking, &BookedQuoteLine)> {
        let (key, suffix) = id.rsplit_once("-L")?;
        let index = suffix.parse::<usize>().ok()?.checked_sub(1)?;
        let booking = self.quote_bookings.get(key)?;
        let line = booking.lines.get(index)?;
        (line.booked.id == id).then_some((booking, line))
    }
    pub(crate) fn validate_recorded_transaction(&self, transaction: &Transaction) -> Result<()> {
        if let Some((_, line)) = self.booking_for_transaction(&transaction.id) {
            ensure(
                transaction == &line.booked,
                "Booked invoice differs from immutable evidence",
            )
        } else {
            self.validate_transaction(transaction)
        }
    }
    pub(crate) fn validate_quote_bookings(&self) -> Result<()> {
        ensure(
            self.quote_bookings.len() <= 1000
                && self
                    .quote_bookings
                    .values()
                    .map(|b| b.lines.len())
                    .sum::<usize>()
                    <= 5000,
            "Booking limit reached (1,000 bookings / 5,000 lines)",
        )?;
        let mut quotes = BTreeSet::new();
        for (key, b) in &self.quote_bookings {
            b.request.validate()?;
            text(&b.owner)?;
            ensure(
                key == &b.request.id
                    && quotes.insert(&b.request.quote_id)
                    && b.created_revision > 0
                    && b.created_revision <= self.revision,
                "Booking identity, revision or unique full-quote constraint invalid",
            )?;
            let q = self
                .quotes
                .get(&b.request.quote_id)
                .ok_or_else(|| error("Booked quote missing"))?;
            let cycle = &b.approval_cycle;
            ensure(
                q.governance
                    .as_ref()
                    .is_some_and(|g| g.cycles.contains(cycle))
                    && cycle.status() == Status::Approved
                    && cycle
                        .submitted_version
                        .checked_add(cycle.reviews.len() as u64)
                        == Some(b.request.quote_version)
                    && b.request.invoice_date >= q.date
                    && b.quote_contributors
                        == self.quote_contributors_at(q, b.created_revision - 1),
                "Booking approved cycle, version, date or frozen contributors mismatch",
            )?;
            let approval = cycle
                .reviews
                .last()
                .ok_or_else(|| error("Booking approval missing"))?;
            let target = quote_target(&q.id);
            ensure(
                approval.revision < b.created_revision
                    && self
                        .audit
                        .iter()
                        .rev()
                        .find(|a| {
                            // Exact legacy author text still identifies a quote mutation if its
                            // target was erased or redirected. It must invalidate old approval.
                            a.revision < b.created_revision
                                && (a.target.as_deref() == Some(&target)
                                    || super::approval::legacy_author(a, &q.id))
                        })
                        .is_some_and(|a| {
                            a.revision == approval.revision && a.command == "quote_decide"
                        }),
                "Quote was not the current approved version when booked",
            )?;
            ensure(
                !b.lines.is_empty() && b.lines.len() == cycle.lines.len(),
                "Booking must contain every approved quote line exactly once",
            )?;
            let mut native_totals: BTreeMap<String, (Money, Money)> = BTreeMap::new();
            for (index, (line, evaluation)) in b.lines.iter().zip(&cycle.lines).enumerate() {
                ensure(
                    line == &derived_line(&b.request, evaluation, index)?,
                    "Booking line, native basis or rounding evidence mismatch",
                )?;
                ensure(
                    self.transactions.get(&line.booked.id) == Some(&line.booked)
                        && self.transaction_revisions.get(&line.booked.id)
                            == Some(&b.created_revision),
                    "Booked invoice is missing or has detached ingestion provenance",
                )?;
                let totals = native_totals
                    .entry(line.native_basis.currency.to_string())
                    .or_insert((Money(0), Money(0)));
                totals.0 = totals.0.add(line.native_basis.invoice_total)?;
                totals.1 = totals.1.add(line.native_basis.cost_total)?;
            }
            let target = booking_target(key);
            let events: Vec<_> = self
                .audit
                .iter()
                .filter(|a| a.target.as_deref() == Some(&target))
                .collect();
            let detail = booking_created_detail(b)?;
            ensure(
                events.len() == 1
                    && events.first().is_some_and(|a| {
                        a.revision == b.created_revision
                            && a.command == "quote_book"
                            && a.actor.name == b.owner
                            && matches!(a.actor.role, Role::Analyst | Role::Admin)
                            && a.detail == detail
                    }),
                "Booking audit, owner or immutable fingerprint mismatch",
            )?;
            ensure(
                self.transaction_revisions
                    .values()
                    .filter(|r| **r == b.created_revision)
                    .count()
                    == b.lines.len(),
                "Booking ingestion contains extra or missing invoices",
            )?;
        }
        for audit in self.audit.iter().filter(|a| {
            a.command == "quote_book"
                || a.target
                    .as_deref()
                    .is_some_and(|t| t.starts_with("booking:"))
        }) {
            let key = audit
                .target
                .as_deref()
                .and_then(|t| t.strip_prefix("booking:"))
                .ok_or_else(|| error("Booking audit target missing"))?;
            ensure(
                self.quote_bookings.contains_key(key),
                "Booking evidence was removed",
            )?;
        }
        Ok(())
    }
}
