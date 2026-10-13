use super::*;

pub(super) fn apply(state: &mut State, actor: &Actor, request: &BookingRequest) -> Result<String> {
    let lines = state.preview_quote_booking(request)?;
    let quote = state
        .quotes
        .get(&request.quote_id)
        .ok_or_else(|| error("Quote not found"))?;
    let cycle = quote
        .governance
        .as_ref()
        .and_then(|g| g.cycles.last())
        .ok_or_else(|| error("Frozen quote approval missing"))?
        .clone();
    let booking = QuoteBooking {
        request: request.clone(),
        owner: actor.name.clone(),
        created_revision: next_version(state.revision)?,
        rounding: BookingRounding::ExactApprovedTotalsNativeUnitDiscountThenExtendV1,
        approval_cycle: cycle,
        quote_contributors: state.quote_contributors(quote),
        lines,
    };
    let detail = booking_created_detail(&booking)?;
    for line in &booking.lines {
        state
            .transaction_revisions
            .insert(line.booked.id.clone(), booking.created_revision);
        state
            .transactions
            .insert(line.booked.id.clone(), line.booked.clone());
    }
    state.quote_bookings.insert(request.id.clone(), booking);
    analytics(state)?;
    Ok(detail)
}
