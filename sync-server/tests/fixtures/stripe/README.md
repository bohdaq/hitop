# Stripe webhook bodies (tests/billing.rs)

Event envelopes shaped like the ones Stripe documents for API version `2026-10-28.endive` (the
version billing.rs pins; checked 2026-11-03):

- the event object: <https://docs.stripe.com/api/events/object> (`id` `evt_…`, `object: "event"`,
  `api_version`, `created`, `livemode`, `type`, `data.object`)
- signatures: <https://docs.stripe.com/webhooks#verify-manually> (`Stripe-Signature: t=…,v1=…`,
  HMAC-SHA256 of `"<t>.<raw body>"` under the endpoint secret, 5-minute tolerance)
- Checkout Session: <https://docs.stripe.com/api/checkout/sessions/object> (`mode`, `subscription`,
  `customer`, `client_reference_id`, `metadata`)
- Subscription: <https://docs.stripe.com/api/subscriptions/object> (since `2025-03-31.basil` the
  period is on the item: `items.data[].current_period_end`)
- Invoice: <https://docs.stripe.com/api/invoices/object> (since basil the subscription is
  `parent.subscription_details.subscription`, with its `metadata`)
- Charge: <https://docs.stripe.com/api/charges/object> (`amount`, `amount_refunded`, `refunded`)
- Managed Payments on Checkout: <https://docs.stripe.com/payments/managed-payments/set-up>

Values (names, addresses, ids) are made up. Each file is a template: the test sets the event id,
type, `created`, ids, status, quantity and metadata it needs, signs the bytes it sends with the
server's clock and posts them. Replace a file with a real test-mode delivery (Dashboard →
Developers → Webhooks → the endpoint → event deliveries) when the owner's account exists; keep
the personal data out.
