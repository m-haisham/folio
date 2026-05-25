use crate::types::*;
use chrono::Local;

use rust_decimal::Decimal;

pub fn compute_invoice(
    invoice: &Invoice,
    client: &Client,
    config: &FolioConfig,
) -> ComputedInvoice {
    let tax_rate = invoice
        .tax_rate
        .or(config.defaults.tax_rate)
        .unwrap_or_default();

    let currency = invoice
        .currency
        .clone()
        .or_else(|| client.currency.clone())
        .or_else(|| config.defaults.currency.clone())
        .unwrap_or_else(|| "USD".to_string());

    let template = invoice
        .template
        .clone()
        .or_else(|| client.template.clone())
        .or_else(|| config.defaults.template.clone())
        .unwrap_or_else(|| "basic".to_string());

    let due_days = client.due_days.or(config.defaults.due_days).unwrap_or(30);

    let due = invoice
        .due
        .unwrap_or_else(|| invoice.date + chrono::Duration::days(due_days as i64));

    let computed_items: Vec<ComputedLineItem> = invoice
        .items
        .iter()
        .map(|item| {
            let total = item.quantity * item.rate;
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
    let tax_amount = subtotal * (tax_rate / Decimal::from(100));
    let total = subtotal + tax_amount;

    let outstanding = if let Some(ref paid) = invoice.paid {
        let remaining = total - paid.amount;
        if remaining < Decimal::ZERO {
            Decimal::ZERO
        } else {
            remaining
        }
    } else if invoice.voided.is_some() {
        Decimal::ZERO
    } else {
        total
    };

    let today = Local::now().date_naive();
    let status = derive_status(invoice, total, &due, &today);

    ComputedInvoice {
        id: invoice.id.clone(),
        client: invoice.client.clone(),
        date: invoice.date,
        due,
        currency,
        template,
        tax_rate,
        notes: invoice.notes.clone(),
        items: computed_items,
        subtotal,
        tax_amount,
        total,
        outstanding,
        status,
        sent: invoice.sent.clone(),
        paid: invoice.paid.clone(),
        voided: invoice.voided.clone(),
    }
}

pub fn derive_status(
    invoice: &Invoice,
    total: Decimal,
    due: &chrono::NaiveDate,
    today: &chrono::NaiveDate,
) -> InvoiceStatus {
    if invoice.voided.is_some() {
        return InvoiceStatus::Voided;
    }
    if let Some(ref paid) = invoice.paid {
        if paid.amount < total {
            return InvoiceStatus::PartiallyPaid;
        }
        return InvoiceStatus::Paid;
    }
    if invoice.sent.is_none() {
        return InvoiceStatus::Draft;
    }
    if due < today {
        InvoiceStatus::Overdue
    } else {
        InvoiceStatus::Sent
    }
}
