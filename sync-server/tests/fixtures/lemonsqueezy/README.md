# Lemon Squeezy webhook bodies (tests/billing.rs)

Shaped like the bodies Lemon Squeezy documents (checked 2026-11-03):

- request format: <https://docs.lemonsqueezy.com/help/webhooks/webhook-requests> (`meta.event_name`,
  `meta.custom_data`, a JSON:API `data` resource; headers `X-Event-Name`, `X-Signature`)
- signature: <https://docs.lemonsqueezy.com/help/webhooks/signing-requests> (hex HMAC-SHA256 of
  the raw body under the webhook's signing secret)
- events and the object each carries: <https://docs.lemonsqueezy.com/help/webhooks/event-types>
- the subscription object: <https://docs.lemonsqueezy.com/api/subscriptions/the-subscription-object>
- the subscription invoice object: <https://docs.lemonsqueezy.com/api/subscription-invoices/the-subscription-invoice-object>
- the order object: <https://docs.lemonsqueezy.com/api/orders/the-order-object>

Values (names, addresses, ids) are made up. Each file is a template: the test sets the event
name, ids, status, quantity, `updated_at` and `custom_data` it needs, signs the bytes it sends and
posts them. Replace a file with a real test-mode delivery (Lemon Squeezy dashboard → Webhooks →
recent deliveries) when the owner's store exists; keep the made-up personal data out.
