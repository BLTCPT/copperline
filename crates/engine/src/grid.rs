use super::*;
pub(super) fn apply(s: &mut State, actor: &Actor, c: &Command) -> Result<String> {
    match c {
        Command::GridCalculate {
            id: key,
            version,
            request,
        } => {
            id(key)?;
            let existing = s.grid.get(key);
            ensure(
                existing.map(|i| i.version).unwrap_or(0) == *version,
                "Grid item version changed",
            )?;
            if let Some(old) = existing {
                ensure(
                    old.status != Status::Pending,
                    "Pending grid candidate is locked until decided",
                )?;
                ensure(
                    old.candidate.request.product_id == request.product_id
                        && old.candidate.request.customer_id == request.customer_id,
                    "Grid product/customer cannot change; create another item",
                )?;
            }
            let candidate = evaluate(s, request)?;
            let item = GridItem {
                id: key.clone(),
                owner: actor.name.clone(),
                version: next_version(*version)?,
                source_revision: s.revision,
                policy: s.policy.clone(),
                status: Status::Draft,
                candidate,
                submitted_by: None,
                decisions: existing.map(|i| i.decisions.clone()).unwrap_or_default(),
                last_approved: existing.and_then(|i| i.last_approved.clone()),
            };
            s.grid.insert(key.clone(), item);
            Ok(format!(
                "Calculated candidate {key}; active price unchanged"
            ))
        }
        Command::GridSubmit { id, version } => {
            let item = s
                .grid
                .get_mut(id)
                .ok_or_else(|| error("Grid item not found"))?;
            ensure(
                item.version == *version && item.status == Status::Draft,
                "Submit the current draft candidate",
            )?;
            ensure(
                !item.candidate.hard_block,
                "Grid candidate violates hard guardrail",
            )?;
            item.status = Status::Pending;
            item.submitted_by = Some(actor.name.clone());
            item.version = next_version(item.version)?;
            Ok(format!("Submitted grid candidate {id}"))
        }
        Command::GridDecide {
            id,
            version,
            approve,
            note,
        } => {
            text(note)?;
            let item = s
                .grid
                .get_mut(id)
                .ok_or_else(|| error("Grid item not found"))?;
            ensure(
                item.version == *version && item.status == Status::Pending,
                "Decision requires current pending grid candidate",
            )?;
            ensure(
                item.owner != actor.name && item.submitted_by.as_ref() != Some(&actor.name),
                "A different person must approve this grid item",
            )?;
            let decision = Decision {
                actor: actor.name.clone(),
                note: note.clone(),
                revision: s.revision + 1,
                approved: *approve,
            };
            if *approve {
                item.last_approved = Some(ApprovedPrice {
                    evaluation: item.candidate.clone(),
                    policy: item.policy.clone(),
                    source_revision: item.source_revision,
                    decision: decision.clone(),
                });
            }
            item.decisions.push(decision);
            item.status = if *approve {
                Status::Approved
            } else {
                Status::Rejected
            };
            item.version = next_version(item.version)?;
            Ok(format!(
                "{} grid candidate {id}",
                if *approve { "Activated" } else { "Rejected" }
            ))
        }
        _ => Err(error("Not a grid command")),
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
    fn recalculation_and_rejection_preserve_active_price() -> Result<()> {
        let mut e = Engine::memory();
        e.run(a("admin", Role::Admin), Command::SeedDemo)?;
        let request = PriceRequest {
            product_id: "VLV-100".into(),
            customer_id: "C-ATLAS".into(),
            date: Date::parse("2026-10-13")?,
            quantity: Quantity(1000),
            discount: Bps(0),
            rebate: Bps(0),
        };
        e.run(
            a("pricing", Role::Analyst),
            Command::GridCalculate {
                id: "G1".into(),
                version: 0,
                request: request.clone(),
            },
        )?;
        e.run(
            a("pricing", Role::Analyst),
            Command::GridSubmit {
                id: "G1".into(),
                version: 1,
            },
        )?;
        let before = e.state().clone();
        assert!(
            e.run(
                a("pricing", Role::Admin),
                Command::GridDecide {
                    id: "G1".into(),
                    version: 2,
                    approve: true,
                    note: "self".into()
                }
            )
            .is_err()
        );
        assert_eq!(e.state(), &before);
        e.run(
            a("reviewer", Role::Approver),
            Command::GridDecide {
                id: "G1".into(),
                version: 2,
                approve: true,
                note: "Reviewed".into(),
            },
        )?;
        let active = e
            .state()
            .grid
            .get("G1")
            .and_then(|i| i.last_approved.clone())
            .ok_or_else(|| error("No active price"))?;
        let mut product = e
            .state()
            .products
            .get("VLV-100")
            .cloned()
            .ok_or_else(|| error("No product"))?;
        product.list_price = Money(12000);
        e.run(a("pricing", Role::Analyst), Command::ProductPut(product))?;
        e.run(
            a("pricing", Role::Analyst),
            Command::GridCalculate {
                id: "G1".into(),
                version: 3,
                request: request.clone(),
            },
        )?;
        let item = e.state().grid.get("G1").ok_or_else(|| error("No grid"))?;
        assert_eq!(item.last_approved, Some(active.clone()));
        assert_eq!(item.candidate.invoice_unit, Money(12000));
        e.run(
            a("pricing", Role::Analyst),
            Command::GridSubmit {
                id: "G1".into(),
                version: 4,
            },
        )?;
        assert!(
            e.run(
                a("pricing", Role::Analyst),
                Command::GridCalculate {
                    id: "G1".into(),
                    version: 5,
                    request
                }
            )
            .is_err()
        );
        e.run(
            a("reviewer", Role::Approver),
            Command::GridDecide {
                id: "G1".into(),
                version: 5,
                approve: false,
                note: "Hold existing price".into(),
            },
        )?;
        assert_eq!(
            e.state()
                .grid
                .get("G1")
                .and_then(|i| i.last_approved.clone()),
            Some(active)
        );
        Ok(())
    }
}
