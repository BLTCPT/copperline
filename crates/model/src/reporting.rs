//! Immutable reporting-currency views of recorded invoice actuals.
use super::*;
use copperline_money::Rational;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportingRequest {
    pub start: Date,
    pub end: Date,
    pub currency: Currency,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConvertedInvoice {
    /// Legacy wire name: the recorded actual, in booked currency/UOM for bookings.
    pub native: Transaction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub booking: Option<BookingReportEvidence>,
    pub reversed_by: Option<String>,
    pub fx: Option<RateEvidence>,
    pub invoice: Money,
    pub cost: Money,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BookingReportEvidence {
    pub booking_id: String,
    pub quote_id: String,
    pub quote_version: u64,
    pub line_index: usize,
    pub booking_revision: u64,
    pub rate_knowledge_revision: u64,
    /// Non-posted pricing basis; never substituted for recorded actuals.
    pub native_basis: NativeBookingBasis,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportingGroup {
    pub customer_id: String,
    pub currency: Currency,
    pub gross_count: usize,
    pub reversal_count: usize,
    pub gross_invoice: Money,
    pub gross_cost: Money,
    pub reversed_invoice: Money,
    pub reversed_cost: Money,
    pub invoice: Money,
    pub cost: Money,
    pub margin: Money,
    pub margin_bps: Option<i32>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportingRounding {
    EachRecordedTotalHalfAwayThenAggregateV1,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConvertedActuals {
    pub rounding: ReportingRounding,
    pub request: ReportingRequest,
    pub source_revision: u64,
    pub rows: Vec<ConvertedInvoice>,
    pub groups: Vec<ReportingGroup>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConversionReport {
    pub id: String,
    pub owner: String,
    pub created_revision: u64,
    pub calculation: ConvertedActuals,
}
pub fn report_target(id: &str) -> String {
    format!("report:{id}")
}
pub fn report_created_detail(report: &ConversionReport) -> Result<String> {
    Ok(format!(
        "Frozen converted actuals {}: {}",
        report.id,
        record_fingerprint(report)?
    ))
}
impl State {
    pub fn converted_actuals(
        &self,
        request: &ReportingRequest,
        revision: u64,
    ) -> Result<ConvertedActuals> {
        request.start.validate()?;
        request.end.validate()?;
        ensure(
            request.start < request.end && revision <= self.revision,
            "Reporting interval or source revision invalid",
        )?;
        let mut rows = vec![];
        let mut groups: BTreeMap<String, ReportingGroup> = BTreeMap::new();
        for t in self.transactions.values().filter(|t| {
            t.date >= request.start
                && t.date < request.end
                && self
                    .transaction_revisions
                    .get(&t.id)
                    .is_some_and(|r| *r <= revision)
        }) {
            ensure(
                rows.len() < 5000,
                "Converted report is limited to 5,000 invoice rows",
            )?;
            let booking = if let Some((b, line)) = self.booking_for_transaction(&t.id) {
                Some(BookingReportEvidence {
                    booking_id: b.request.id.clone(),
                    quote_id: b.request.quote_id.clone(),
                    quote_version: b.request.quote_version,
                    line_index: line.index,
                    booking_revision: b.created_revision,
                    rate_knowledge_revision: b
                        .created_revision
                        .checked_sub(1)
                        .ok_or_else(|| error("Invalid booking revision in report"))?,
                    native_basis: line.native_basis.clone(),
                })
            } else {
                None
            };
            let rate_revision = booking
                .as_ref()
                .map(|b| b.rate_knowledge_revision)
                .unwrap_or(revision);
            let fx = if t.currency == request.currency {
                None
            } else {
                Some(self.conversion_rate_at(
                    &ConversionKind::Fx {
                        from: t.currency,
                        to: request.currency,
                    },
                    &t.date,
                    rate_revision,
                ).map_err(|e| if booking.is_some() {
                    error(format!("Reporting FX for booked invoice {} was not approved when booked: {e}", t.id))
                } else { e })?)
            };
            let ratio = fx.as_ref().map(|r| r.entry.ratio).unwrap_or(Rational::ONE);
            let invoice = ratio.money(t.invoice_total)?;
            let cost = ratio.money(t.cost_total)?;
            let reversals: Vec<_> = self
                .corrections
                .values()
                .filter(|c| {
                    c.proposal.original_transaction_id == t.id
                        && c.approved_revision().is_some_and(|r| r <= revision)
                })
                .collect();
            ensure(
                reversals.len() <= 1,
                "Invoice has multiple approved reversals",
            )?;
            let reversed_by = reversals.first().map(|c| c.proposal.id.clone());
            let group = groups
                .entry(t.customer_id.clone())
                .or_insert(ReportingGroup {
                    customer_id: t.customer_id.clone(),
                    currency: request.currency,
                    gross_count: 0,
                    reversal_count: 0,
                    gross_invoice: Money(0),
                    gross_cost: Money(0),
                    reversed_invoice: Money(0),
                    reversed_cost: Money(0),
                    invoice: Money(0),
                    cost: Money(0),
                    margin: Money(0),
                    margin_bps: None,
                });
            group.gross_count += 1;
            group.gross_invoice = group.gross_invoice.add(invoice)?;
            group.gross_cost = group.gross_cost.add(cost)?;
            if reversed_by.is_some() {
                group.reversal_count += 1;
                group.reversed_invoice = group.reversed_invoice.add(invoice)?;
                group.reversed_cost = group.reversed_cost.add(cost)?;
            }
            rows.push(ConvertedInvoice {
                native: t.clone(),
                booking,
                reversed_by,
                fx,
                invoice,
                cost,
            });
        }
        for g in groups.values_mut() {
            g.invoice = g.gross_invoice.sub(g.reversed_invoice)?;
            g.cost = g.gross_cost.sub(g.reversed_cost)?;
            g.margin = g.invoice.sub(g.cost)?;
            g.margin_bps = if g.invoice.0 > 0 {
                Some(
                    i32::try_from(copperline_money::ratio(g.margin.0, 10_000, g.invoice.0)?)
                        .map_err(|_| error("Reporting margin overflow"))?,
                )
            } else {
                None
            };
        }
        Ok(ConvertedActuals {
            rounding: ReportingRounding::EachRecordedTotalHalfAwayThenAggregateV1,
            request: request.clone(),
            source_revision: revision,
            rows,
            groups: groups.into_values().collect(),
        })
    }
    pub(crate) fn validate_conversion_reports(&self) -> Result<()> {
        ensure(
            self.conversion_reports.len() <= 100
                && self
                    .conversion_reports
                    .values()
                    .map(|r| r.calculation.rows.len())
                    .sum::<usize>()
                    <= 5000,
            "Frozen report/row limit reached",
        )?;
        for (key, r) in &self.conversion_reports {
            id(key)?;
            text(&r.owner)?;
            ensure(
                key == &r.id
                    && r.created_revision > 0
                    && r.created_revision <= self.revision
                    && r.calculation.source_revision == r.created_revision - 1,
                "Frozen report identity/revision invalid",
            )?;
            ensure(
                r.calculation
                    == self
                        .converted_actuals(&r.calculation.request, r.calculation.source_revision)?,
                "Frozen report differs from historical invoices/rates",
            )?;
            let target = report_target(key);
            let events: Vec<_> = self
                .audit
                .iter()
                .filter(|a| a.target.as_deref() == Some(&target))
                .collect();
            ensure(
                events.len() == 1
                    && events.first().is_some_and(|a| {
                        a.revision == r.created_revision
                            && a.command == "report_convert"
                            && a.actor.name == r.owner
                            && matches!(a.actor.role, Role::Analyst | Role::Admin)
                    }),
                "Frozen report audit/owner invalid",
            )?;
            let detail = report_created_detail(r)?;
            ensure(
                events.first().is_some_and(|a| a.detail == detail),
                "Frozen report evidence fingerprint mismatch",
            )?;
        }
        for a in self.audit.iter().filter(|a| {
            a.command == "report_convert"
                || a.target
                    .as_deref()
                    .is_some_and(|t| t.starts_with("report:"))
        }) {
            let key = a
                .target
                .as_deref()
                .and_then(|t| t.strip_prefix("report:"))
                .ok_or_else(|| error("Report target missing"))?;
            ensure(
                self.conversion_reports.contains_key(key),
                "Frozen report was removed",
            )?;
        }
        Ok(())
    }
}
