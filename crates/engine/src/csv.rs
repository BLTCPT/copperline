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
        "quote_id,quote_version,source_revision,status,product_id,product_name,customer_id,date,quantity,unit,currency,rule,cost_version,list_unit,invoice_unit,invoice_total,rebate_total,pocket_total,cost_total,margin_total\r\n",
    );
    for l in &q.lines {
        let fields = [
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
            l.rule.clone().unwrap_or_else(|| "Catalog list".into()),
            l.cost_version.to_string(),
            l.list_unit.to_string(),
            l.invoice_unit.to_string(),
            l.invoice_total.to_string(),
            l.rebate_total.to_string(),
            l.pocket_total.to_string(),
            l.cost_total.to_string(),
            l.margin_total.to_string(),
        ];
        output.push_str(&fields.iter().map(|s| safe(s)).collect::<Vec<_>>().join(","));
        output.push_str("\r\n");
    }
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
