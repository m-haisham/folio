use crate::types::*;
use chrono::Local;
use rust_decimal::Decimal;

pub fn compute_quote(quote: &Quote, client: &Client, config: &FolioConfig) -> ComputedQuote {
    let tax_rate = quote
        .tax_rate
        .or(config.defaults.tax_rate)
        .unwrap_or_default();

    let currency = quote
        .currency
        .clone()
        .or_else(|| client.currency.clone())
        .or_else(|| config.defaults.currency.clone())
        .unwrap_or_else(|| "USD".to_string());

    let template = quote
        .template
        .clone()
        .or_else(|| client.template.clone())
        .or_else(|| config.quote.as_ref().and_then(|q| q.template.clone()))
        .or_else(|| config.defaults.template.clone())
        .unwrap_or_else(|| "basic".to_string());

    let primary_color = quote
        .primary_color
        .clone()
        .or_else(|| config.defaults.primary_color.clone());

    let notes = quote
        .notes
        .clone()
        .or_else(|| client.defaults.as_ref().and_then(|d| d.notes.clone()))
        .or_else(|| config.defaults.notes.clone());

    let expires_days = config
        .quote
        .as_ref()
        .and_then(|q| q.expires_days)
        .or(config.defaults.expires_days)
        .unwrap_or(30);
    let expires = quote
        .expires
        .unwrap_or_else(|| quote.date + chrono::Duration::days(expires_days as i64));

    let computed_items: Vec<ComputedLineItem> = quote
        .items
        .iter()
        .map(|item| {
            let total = (item.quantity * item.rate).round_dp(2);
            ComputedLineItem {
                description: item.description.clone(),
                quantity: item.quantity,
                unit: item.unit.clone(),
                rate: item.rate,
                total,
            }
        })
        .collect();

    let subtotal: Decimal = computed_items.iter().map(|i| i.total).sum();
    let tax_amount = (subtotal * (tax_rate / Decimal::from(100))).round_dp(2);
    let total = (subtotal + tax_amount).round_dp(2);
    let subtotal = subtotal.round_dp(2);

    let today = Local::now().date_naive();
    let status = derive_quote_status(quote, &expires, &today);

    ComputedQuote {
        id: quote.id.clone(),
        client: quote.client.clone(),
        date: quote.date,
        expires,
        currency,
        template,
        primary_color,
        tax_rate,
        notes,
        items: computed_items,
        subtotal,
        tax_amount,
        total,
        status,
        sent: quote.sent.clone(),
        accepted: quote.accepted.clone(),
        declined: quote.declined.clone(),
    }
}

pub fn derive_quote_status(
    quote: &Quote,
    expires: &chrono::NaiveDate,
    today: &chrono::NaiveDate,
) -> QuoteStatus {
    if quote.accepted.is_some() {
        return QuoteStatus::Accepted;
    }
    if quote.declined.is_some() {
        return QuoteStatus::Declined;
    }
    if quote.sent.is_none() {
        return QuoteStatus::Draft;
    }
    if expires < today {
        QuoteStatus::Expired
    } else {
        QuoteStatus::Sent
    }
}
