---
path: "/legal/privacy.html"
title: "HITOP Cloud privacy notice"
description: "What HITOP Cloud keeps about you, why, for how long, who else handles it, and your rights under the GDPR."
---
# HITOP Cloud privacy notice

This notice adds to the [HITOP privacy policy](/privacy.html), which covers the apps, the web app's proxy and the usage ping. It covers HITOP Cloud: the hosted sync service and the portal.

**Controller:** Bohdan Tsap, [bohdaq@gmail.com](mailto:bohdaq@gmail.com). For an organization on Business or Enterprise, the organization is the controller of its members' workspace data and we process it for them under the [DPA](/legal/dpa.html).

## What we keep and why

| Data | Why (GDPR basis) | Kept |
| --- | --- | --- |
| Email address, device names and public keys, session token hashes, a password hash if you set one | to run your account (contract) | until you delete the account |
| Workspace ciphertext, wrapped keys, sizes and change times | to sync (contract); we can't read the content | until you delete the workspace or account |
| Your personal library, when you are signed in to the app: your own collections, environments and globals as you saved them (the server can read them); their secret values encrypted at rest under a server key | to keep your work in every browser you sign in to (contract) | until you delete them or the account |
| Organizations, members, roles, plan, seats, trial dates | to run the plan you chose (contract) | while the organization exists |
| Stripe customer, subscription, price and checkout ids, plan status; ids and outcomes of its webhook events (not their content) | billing (contract, legal duty to keep records) | as long as tax law requires |
| Security log: sign-ins, member and key changes, IP address cut to its first three parts, user agent | security of the service (legitimate interest) | one year |
| Run status rows from your own runners (pass or fail, timings, counts) and alert settings, once run history opens | to show history and send alerts (contract) | the plan's history period |

We keep no request bodies, no analytics and no advertising data, and we sell nothing.

## Payments

Stripe (merchant of record through Stripe Managed Payments; the checkout says "Sold through Link") collects your name, email, billing address, payment details and tax information when you buy, under [its own privacy policy](https://stripe.com/privacy) and [Link's](https://link.com/privacy). We receive the subscription status and ids, never the card.

## Who else handles data

The [sub-processor list](/legal/subprocessors.html) names each company and what it sees. Data stays in the EU, except what Stripe and the mail service process under their own safeguards.

## Backups

Encrypted backups are kept for 35 days. Something you delete is gone from the live service at once and from backups when they expire.

## Your rights

You may ask for a copy of your data, its correction or deletion, a restriction, or object to processing, and complain to your data protection authority. Export your workspaces yourself with `hitop sync pull`; mail us for the rest. We answer within 30 days.
