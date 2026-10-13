use copperline_model::*;
use copperline_money::Money;
use std::collections::BTreeSet;
pub(super) fn rows(input: &str) -> Result<Vec<Vec<String>>> {
    ensure(input.len() <= 1024 * 1024, "CSV import limit is 1 MiB")?;
    let mut rows = vec![];
    let mut row = vec![];
    let mut cell = String::new();
    let mut chars = input.trim_start_matches('\u{feff}').chars().peekable();
    let mut quoted = false;
    let mut closed = false;
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    cell.push('"');
                } else {
                    quoted = false;
                    closed = true;
                }
            } else {
                cell.push(c);
            }
        } else {
            match c {
                '"' => {
                    ensure(cell.is_empty() && !closed, "Unexpected quote in CSV")?;
                    quoted = true;
                }
                ',' => {
                    row.push(std::mem::take(&mut cell));
                    closed = false;
                }
                '\n' | '\r' => {
                    if c == '\r' && chars.peek() == Some(&'\n') {
                        chars.next();
                    }
                    row.push(std::mem::take(&mut cell));
                    rows.push(std::mem::take(&mut row));
                    closed = false;
                }
                _ => {
                    ensure(!closed, "Characters after closing CSV quote")?;
                    cell.push(c);
                }
            }
        }
        ensure(
            cell.len() <= 2048 && row.len() <= 16 && rows.len() <= 10_001,
            "CSV row, field or column limit exceeded",
        )?;
    }
    ensure(!quoted, "Unterminated quoted CSV field")?;
    if !cell.is_empty() || !row.is_empty() || closed {
        row.push(cell);
        rows.push(row);
    }
    Ok(rows)
}
pub fn import_products(input: &str) -> Result<Vec<Product>> {
    let rows = rows(input)?;
    let header = rows.first().ok_or_else(|| error("CSV is empty"))?;
    ensure(
        header == &["id", "name", "family", "unit", "currency", "list_price"],
        "Expected header: id,name,family,unit,currency,list_price",
    )?;
    let mut products = vec![];
    let mut seen = BTreeSet::new();
    for (row_num, row) in rows.iter().enumerate().skip(1) {
        ensure(
            row.len() == 6,
            &format!("Row {} must contain 6 columns", row_num + 1),
        )?;
        let field = |i| {
            row.get(i)
                .cloned()
                .ok_or_else(|| error("Missing CSV field"))
        };
        let currency = match field(4)?.as_str() {
            "USD" => Currency::USD,
            "EUR" => Currency::EUR,
            "GBP" => Currency::GBP,
            _ => {
                return Err(error(format!(
                    "Row {}: supported currencies USD, EUR, GBP",
                    row_num + 1
                )));
            }
        };
        let p = Product {
            id: field(0)?,
            name: field(1)?,
            family: field(2)?,
            unit: field(3)?,
            currency,
            list_price: field(5)?.parse::<Money>()?,
        };
        p.validate()
            .map_err(|e| error(format!("Row {}: {e}", row_num + 1)))?;
        ensure(
            seen.insert(p.id.clone()),
            &format!("Row {}: duplicate product ID {}", row_num + 1, p.id),
        )?;
        products.push(p);
    }
    ensure(!products.is_empty(), "No product rows in CSV")?;
    Ok(products)
}
fn safe(s: &str) -> String {
    let prefix = if s.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        "'"
    } else {
        ""
    };
    format!("\"{}{}\"", prefix, s.replace('"', "\"\""))
}
pub fn export_quote(q: &Quote) -> Result<String> {
    ensure(
        q.status == Status::Approved,
        "Only approved quotes can be exported as a commercial CSV",
    )?;
    let mut output = String::from(
        "quote_id,quote_version,source_revision,status,product_id,product_name,customer_id,date,quantity,unit,currency,rule,cost_version,list_unit,invoice_unit,invoice_total,rebate_total,pocket_total,cost_total,margin_total,pricing_source,pricing_source_version,pricing_source_revision,pricing_source_approval_revision,pricing_source_publication_revision,pricing_source_captured_revision,pricing_source_invoice_unit,pricing_source_min_quantity\r\n",
    );
    let contracts = q
        .lines
        .iter()
        .any(|l| l.source.as_ref().is_some_and(|s| s.contract.is_some()));
    if contracts {
        output.truncate(output.len().saturating_sub(2));
        output.push_str(",contract_selection,contract_row,contract_start,contract_end_exclusive,contract_max_quantity_exclusive,contract_priority,contract_fingerprint\r\n");
    }
    let conversions = q.lines.iter().any(|l| l.conversion.is_some());
    if conversions {
        output.truncate(output.len().saturating_sub(2));
        output.push_str(",native_currency,native_unit,native_quantity,native_list_unit,native_rule_unit,native_cost_unit,conversion_rounding,fx_evidence_json,uom_evidence_json\r\n");
    }
    for l in &q.lines {
        let mut fields = vec![
            q.id.clone(),
            q.version.to_string(),
            q.source_revision.to_string(),
            format!("{:?}", q.status),
            l.request.product_id.clone(),
            l.product_name.clone(),
            q.customer_id.clone(),
            q.date.to_string(),
            l.request.quantity.to_string(),
            l.unit.clone(),
            l.currency.to_string(),
            l.rule.clone().unwrap_or_else(|| {
                if l.source.is_some() {
                    "Explicit approved source".into()
                } else {
                    "Catalog list".into()
                }
            }),
            l.cost_version.to_string(),
            l.list_unit.to_string(),
            l.invoice_unit.to_string(),
            l.invoice_total.to_string(),
            l.rebate_total.to_string(),
            l.pocket_total.to_string(),
            l.cost_total.to_string(),
            l.margin_total.to_string(),
            l.source
                .as_ref()
                .map(|s| s.source.label())
                .unwrap_or_else(|| "Live catalog/rules".into()),
            l.source
                .as_ref()
                .and_then(|s| s.document_version)
                .map(|v| v.to_string())
                .unwrap_or_default(),
            l.source
                .as_ref()
                .map(|s| s.pricing_revision.to_string())
                .unwrap_or_default(),
            l.source
                .as_ref()
                .map(|s| s.approval_revision.to_string())
                .unwrap_or_default(),
            l.source
                .as_ref()
                .and_then(|s| s.publication_revision)
                .map(|v| v.to_string())
                .unwrap_or_default(),
            l.source
                .as_ref()
                .map(|s| s.captured_revision.to_string())
                .unwrap_or_default(),
            l.source
                .as_ref()
                .map(|s| s.invoice_unit.to_string())
                .unwrap_or_default(),
            l.source
                .as_ref()
                .map(|s| s.min_quantity.to_string())
                .unwrap_or_default(),
        ];
        if contracts {
            let c = l.source.as_ref().and_then(|s| s.contract.as_ref());
            fields.extend([
                c.map(|_| {
                    if l.request.source == Some(PriceSource::CustomerContract) {
                        "automatic"
                    } else {
                        "exact"
                    }
                    .into()
                })
                .unwrap_or_default(),
                c.map(|c| c.row_id.clone()).unwrap_or_default(),
                c.map(|c| c.start.to_string()).unwrap_or_default(),
                c.map(|c| c.end.to_string()).unwrap_or_default(),
                c.and_then(|c| c.max_quantity)
                    .map(|q| q.to_string())
                    .unwrap_or_default(),
                c.map(|c| c.priority.to_string()).unwrap_or_default(),
                c.map(|c| c.terms_fingerprint.clone()).unwrap_or_default(),
            ]);
        }
        if conversions {
            let c = l.conversion.as_ref();
            fields.extend([
                c.map(|c| c.native_currency.to_string())
                    .unwrap_or_else(|| l.currency.to_string()),
                c.map(|c| c.native_unit.clone())
                    .unwrap_or_else(|| l.unit.clone()),
                c.map(|c| c.native_quantity.to_string())
                    .unwrap_or_else(|| l.request.quantity.to_string()),
                c.map(|c| c.native_list_unit.to_string())
                    .unwrap_or_else(|| l.list_unit.to_string()),
                c.map(|c| c.native_rule_unit.to_string())
                    .unwrap_or_else(|| l.rule_unit.to_string()),
                c.map(|c| c.native_cost_unit.to_string())
                    .unwrap_or_else(|| l.cost_unit.to_string()),
                c.map(|_| "native_unit_combined_half_away_then_extend_v1".into())
                    .unwrap_or_default(),
                json_cell(&c.and_then(|c| c.fx.as_ref()))?,
                json_cell(&c.and_then(|c| c.uom.as_ref()))?,
            ]);
        }
        output.push_str(&fields.iter().map(|s| safe(s)).collect::<Vec<_>>().join(","));
        output.push_str("\r\n");
    }
    Ok(output)
}
/// Frozen approved local settlement evidence. Never a payment instruction.
pub fn export_settlement(state: &State, id: &str) -> Result<String> {
    let record = state
        .settlements
        .get(id)
        .ok_or_else(|| error("Settlement not found"))?;
    ensure(
        record.status == RecordStatus::Approved,
        "Only approved settlements can export",
    )?;
    let review = record
        .review
        .as_ref()
        .ok_or_else(|| error("Settlement review missing"))?;
    let accrual = state
        .accruals
        .get(&record.accrual_id)
        .ok_or_else(|| error("Settlement accrual missing"))?;
    let mut fields = vec![
        record.id.clone(),
        record.version.to_string(),
        record.agreement_id.clone(),
        record.agreement_version.to_string(),
        record.accrual_id.clone(),
        accrual.source_revision.to_string(),
        record.currency.to_string(),
        record.target_rebate.to_string(),
        record.previously_recognized.to_string(),
        record.adjustment.to_string(),
        record.prior_settlement_ids.join("|"),
        accrual.transaction_ids.join("|"),
        accrual.correction_ids.join("|"),
        record.owner.clone(),
        review.actor.name.clone(),
        review.revision.to_string(),
        record.reason.clone(),
        "Local recognition only; no payment or posting".into(),
    ];
    let mut header = String::from(
        "settlement_id,version,agreement_id,agreement_version,accrual_id,accrual_source_revision,currency,target_rebate,previously_recognized,adjustment,prior_settlement_ids,invoice_ids,correction_ids,owner,finance_reviewer,approval_revision,reason,boundary\n",
    );
    if !accrual.credit_ids.is_empty() {
        header = header.trim_end().to_owned();
        header.push_str(",credit_ids\n");
        fields.push(accrual.credit_ids.join("|"));
    }
    Ok(format!(
        "{header}{}\n",
        fields
            .iter()
            .enumerate()
            .map(|(i, s)| if matches!(i, 1 | 3 | 5 | 7 | 8 | 9 | 15) {
                format!("\"{s}\"")
            } else {
                safe(s)
            })
            .collect::<Vec<_>>()
            .join(",")
    ))
}

/// Immutable approved contractual terms, independent of later edition activation.
pub fn export_contract(state: &State, id: &str, edition: u64) -> Result<String> {
    let c = state.contract(id, edition)?;
    ensure(
        c.status == RecordStatus::Approved,
        "Only approved contract editions may export",
    )?;
    let review = c
        .review
        .as_ref()
        .ok_or_else(|| error("Contract approval missing"))?;
    let fingerprint = contract_fingerprint(c)?;
    let mut output = String::from(
        "contract_id,edition,name,customer_id,currency,start,end_exclusive,priority,row_id,product_id,unit,min_quantity,max_quantity_exclusive,invoice_unit,origin,origin_capture_revision,origin_approval_revision,owner,reviewer,approval_revision,terms_fingerprint\r\n",
    );
    for row in &c.terms.rows {
        let frozen = c
            .rows
            .iter()
            .find(|r| r.row_id == row.id)
            .ok_or_else(|| error("Frozen contract row missing"))?;
        let fields = [
            c.terms.id.clone(),
            edition.to_string(),
            c.terms.name.clone(),
            c.terms.customer_id.clone(),
            c.terms.currency.to_string(),
            c.terms.start.to_string(),
            c.terms.end.to_string(),
            c.terms.priority.to_string(),
            row.id.clone(),
            row.product_id.clone(),
            row.unit.clone(),
            row.min_quantity.to_string(),
            row.max_quantity.map(|q| q.to_string()).unwrap_or_default(),
            frozen.invoice_unit.to_string(),
            frozen
                .origin
                .as_ref()
                .map(|o| o.source.label())
                .unwrap_or_else(|| "Fixed approved contract price".into()),
            frozen
                .origin
                .as_ref()
                .map(|o| o.captured_revision.to_string())
                .unwrap_or_default(),
            frozen
                .origin
                .as_ref()
                .map(|o| o.approval_revision.to_string())
                .unwrap_or_default(),
            c.owner.clone(),
            review.actor.name.clone(),
            review.revision.to_string(),
            fingerprint.clone(),
        ];
        output.push_str(
            &fields
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    if matches!(i, 1 | 7 | 11 | 12 | 13 | 15 | 16 | 19) {
                        format!("\"{s}\"")
                    } else {
                        safe(s)
                    }
                })
                .collect::<Vec<_>>()
                .join(","),
        );
        output.push_str("\r\n");
    }
    Ok(output)
}

pub fn import_conversions(input: &str) -> Result<Vec<ConversionEntry>> {
    let rows = rows(input)?;
    ensure(
        rows.first().is_some_and(|r| {
            r == &[
                "id",
                "kind",
                "product_id",
                "from",
                "to",
                "start",
                "end",
                "numerator",
                "denominator",
            ]
        }),
        "Expected header: id,kind,product_id,from,to,start,end,numerator,denominator",
    )?;
    let currency = |s: &str| -> Result<Currency> {
        match s {
            "USD" => Ok(Currency::USD),
            "EUR" => Ok(Currency::EUR),
            "GBP" => Ok(Currency::GBP),
            _ => Err(error("Supported currencies: USD, EUR, GBP")),
        }
    };
    let mut result = vec![];
    for (n, row) in rows.iter().enumerate().skip(1) {
        ensure(
            row.len() == 9,
            &format!("Row {} must contain 9 columns", n + 1),
        )?;
        let f = |i: usize| {
            row.get(i)
                .map(String::as_str)
                .ok_or_else(|| error("Missing conversion field"))
        };
        let conversion = match f(1)? {
            "fx" => {
                ensure(f(2)?.is_empty(), "FX rows must omit product_id")?;
                ConversionKind::Fx {
                    from: currency(f(3)?)?,
                    to: currency(f(4)?)?,
                }
            }
            "uom" => ConversionKind::Uom {
                product_id: f(2)?.into(),
                from_unit: f(3)?.into(),
                to_unit: f(4)?.into(),
            },
            _ => return Err(error("Conversion kind must be fx or uom")),
        };
        let entry = ConversionEntry {
            id: f(0)?.into(),
            conversion,
            start: Date::parse(f(5)?)?,
            end: Date::parse(f(6)?)?,
            ratio: copperline_money::Rational {
                numerator: f(7)?
                    .parse()
                    .map_err(|_| error("Numerator must be an integer"))?,
                denominator: f(8)?
                    .parse()
                    .map_err(|_| error("Denominator must be an integer"))?,
            },
        };
        entry.validate()?;
        result.push(entry);
    }
    ensure(
        !result.is_empty() && result.len() <= 100,
        "Import 1–100 conversion rows",
    )?;
    Ok(result)
}

fn json_cell<T: serde::Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value).map_err(|e| error(e.to_string()))
}
fn csv_row(output: &mut String, fields: &[String]) {
    output.push_str(&fields.iter().map(|s| safe(s)).collect::<Vec<_>>().join(","));
    output.push_str("\r\n");
}
pub fn export_conversion(state: &State, id: &str) -> Result<String> {
    let book = state
        .conversion_books
        .get(id)
        .ok_or_else(|| error("Conversion book not found"))?;
    ensure(
        book.status == RecordStatus::Approved,
        "Only approved conversion books can export",
    )?;
    let review = book
        .review
        .as_ref()
        .ok_or_else(|| error("Conversion approval missing"))?;
    let mut output = String::from(
        "book_id,book_version,owner,approval_revision,approver,fingerprint,row_id,direction,start,end_exclusive,numerator,denominator,entry_json\r\n",
    );
    for row in &book.terms.entries {
        csv_row(
            &mut output,
            &[
                book.terms.id.clone(),
                book.version.to_string(),
                book.owner.clone(),
                review.revision.to_string(),
                review.actor.name.clone(),
                conversion_fingerprint(book)?,
                row.id.clone(),
                row.conversion.label(),
                row.start.to_string(),
                row.end.to_string(),
                row.ratio.numerator.to_string(),
                row.ratio.denominator.to_string(),
                json_cell(row)?,
            ],
        );
    }
    Ok(output)
}
pub fn export_report(state: &State, id: &str) -> Result<String> {
    let report = state
        .conversion_reports
        .get(id)
        .ok_or_else(|| error("Frozen report not found"))?;
    let c = &report.calculation;
    let has_bookings = c.rows.iter().any(|r| r.booking.is_some());
    let has_credits = c.rows.iter().any(|r| !r.credits.is_empty());
    let mut output = String::from(
        "report_id,source_revision,start,end_exclusive,reporting_currency,invoice_id,customer_id,product_id,invoice_date,native_quantity,native_unit,native_currency,native_invoice,native_cost,converted_invoice,converted_cost,reversed_by,net_invoice,net_cost,net_margin,rounding,fx_evidence_json\r\n",
    );
    if has_bookings {
        output = output.replace("native_", "recorded_").trim_end().to_owned();
        output.push_str(",booking_evidence_json\r\n");
    }
    if has_credits {
        output = output.trim_end().to_owned();
        output.push_str(",credit_evidence_json\r\n");
    }
    for row in &c.rows {
        let t = &row.native;
        let reversed = row.reversed_by.is_some();
        let credited_invoice = row
            .credits
            .iter()
            .try_fold(Money(0), |total, c| total.add(c.invoice))?;
        let credited_cost = row
            .credits
            .iter()
            .try_fold(Money(0), |total, c| total.add(c.cost))?;
        let invoice = if reversed {
            Money(0)
        } else {
            row.invoice.sub(credited_invoice)?
        };
        let cost = if reversed {
            Money(0)
        } else {
            row.cost.sub(credited_cost)?
        };
        let mut fields = vec![
            id.into(),
            c.source_revision.to_string(),
            c.request.start.to_string(),
            c.request.end.to_string(),
            c.request.currency.to_string(),
            t.id.clone(),
            t.customer_id.clone(),
            t.product_id.clone(),
            t.date.to_string(),
            t.quantity.to_string(),
            t.unit.clone(),
            t.currency.to_string(),
            t.invoice_total.to_string(),
            t.cost_total.to_string(),
            row.invoice.to_string(),
            row.cost.to_string(),
            row.reversed_by.clone().unwrap_or_default(),
            invoice.to_string(),
            cost.to_string(),
            invoice.sub(cost)?.to_string(),
            "each_recorded_total_half_away_then_aggregate_v1".into(),
            json_cell(&row.fx)?,
        ];
        if has_bookings {
            fields.push(json_cell(&row.booking)?);
        }
        if has_credits {
            fields.push(json_cell(&row.credits)?);
        }
        csv_row(&mut output, &fields);
    }
    Ok(output)
}

/// Immutable booked amounts and non-posted native pricing evidence; no live repricing.
pub fn export_booking(state: &State, id: &str) -> Result<String> {
    let b = state
        .quote_bookings
        .get(id)
        .ok_or_else(|| error("Booking not found"))?;
    let review = b
        .approval_cycle
        .reviews
        .last()
        .ok_or_else(|| error("Booking approval missing"))?;
    let fingerprint = record_fingerprint(b)?;
    let mut output = String::from(
        "booking_id,source,quote_id,quote_version,quote_submitted_revision,quote_approval_revision,booking_revision,reporting_rate_knowledge_revision,owner,line_index,transaction_id,customer_id,product_id,invoice_date,booked_quantity,booked_unit,booked_currency,booked_invoice,booked_cost,native_nonposted_basis_json,booked_minus_forward_converted_native_basis_json,approved_line_json,booking_fingerprint,rounding\r\n",
    );
    for line in &b.lines {
        let t = &line.booked;
        let approved = b
            .approval_cycle
            .lines
            .get(line.index)
            .ok_or_else(|| error("Booked line index missing"))?;
        csv_row(
            &mut output,
            &[
                b.request.id.clone(),
                b.request.source.clone(),
                b.request.quote_id.clone(),
                b.request.quote_version.to_string(),
                b.approval_cycle.submitted_revision.to_string(),
                review.revision.to_string(),
                b.created_revision.to_string(),
                b.created_revision
                    .checked_sub(1)
                    .ok_or_else(|| error("Invalid booking revision"))?
                    .to_string(),
                b.owner.clone(),
                line.index.to_string(),
                t.id.clone(),
                t.customer_id.clone(),
                t.product_id.clone(),
                t.date.to_string(),
                t.quantity.to_string(),
                t.unit.clone(),
                t.currency.to_string(),
                t.invoice_total.to_string(),
                t.cost_total.to_string(),
                json_cell(&line.native_basis)?,
                json_cell(&line.reconciliation)?,
                json_cell(approved)?,
                fingerprint.clone(),
                json_cell(&b.rounding)?,
            ],
        );
    }
    Ok(output)
}

/// Approved quantity credit; complete original/native/baseline evidence is embedded.
pub fn export_credit(state: &State, id: &str) -> Result<String> {
    let c = state
        .credits
        .get(id)
        .ok_or_else(|| error("Credit not found"))?;
    ensure(
        c.status == RecordStatus::Approved,
        "Only approved credits may export",
    )?;
    let review = c
        .review
        .as_ref()
        .ok_or_else(|| error("Credit approval missing"))?;
    let mut output = String::from(
        "credit_id,version,original_invoice_id,original_invoice_date,record_date,recorded_currency,recorded_unit,quantity,prior_quantity,credited_invoice,credited_cost,owner,finance_reviewer,approval_revision,rounding,evidence_json,boundary\r\n",
    );
    csv_row(&mut output, &[c.proposal.id.clone(),c.version.to_string(),c.original.id.clone(),c.original.date.to_string(),c.proposal.effective_date.to_string(),c.original.currency.to_string(),c.original.unit.clone(),c.proposal.quantity.to_string(),c.prior_quantity.to_string(),c.invoice.to_string(),c.cost.to_string(),c.owner.clone(),review.actor.name.clone(),review.revision.to_string(),json_cell(&c.rounding)?,json_cell(c)?,"Commercial quantity credit; original-period eligibility; no accounting posting or payment".into()]);
    Ok(output)
}

/// Approved nominal invoice basis; reporting FX and rebates remain separate.
pub fn export_period_close(state: &State, id: &str) -> Result<String> {
    let c = state
        .period_closes
        .get(id)
        .ok_or_else(|| error("Period close not found"))?;
    ensure(
        c.status == RecordStatus::Approved,
        "Only approved period closes may export",
    )?;
    let review = c
        .review
        .as_ref()
        .ok_or_else(|| error("Period close approval missing"))?;
    let b = &c.basis;
    let mut output = String::from(
        "close_id,version,start,end_exclusive,currency,source_revision,gross_invoice,gross_cost,reversed_invoice,reversed_cost,credited_invoice,credited_cost,net_invoice,net_cost,owner,finance_reviewer,approval_revision,evidence_json,boundary\r\n",
    );
    csv_row(&mut output, &[c.terms.id.clone(),c.version.to_string(),c.terms.start.to_string(),c.terms.end.to_string(),c.terms.currency.to_string(),c.source_revision.to_string(),b.gross_invoice.to_string(),b.gross_cost.to_string(),b.reversed_invoice.to_string(),b.reversed_cost.to_string(),b.credited_invoice.to_string(),b.credited_cost.to_string(),b.invoice.to_string(),b.cost.to_string(),c.owner.clone(),review.actor.name.clone(),review.revision.to_string(),json_cell(c)?,"Nominal invoice-basis freeze only; excludes FX/rebates; no accounting-period compliance assertion".into()]);
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn csv_quoted_and_errors() -> Result<()> {
        let p = import_products(
            "id,name,family,unit,currency,list_price\r\nP,\"Valve, precision\",Flow,each,USD,100.00\r\n",
        )?;
        assert_eq!(p.first().map(|p| p.name.as_str()), Some("Valve, precision"));
        assert!(
            import_products(
                "id,name,family,unit,currency,list_price\nP,A,F,each,USD,1\nP,B,F,each,USD,2"
            )
            .is_err()
        );
        assert!(rows("\"unterminated").is_err());
        assert_eq!(safe("=1+1"), "\"'=1+1\"");
        Ok(())
    }
}
