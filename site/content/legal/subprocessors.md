---
path: "/legal/subprocessors.html"
title: "Sub-processors · HITOP Cloud"
description: "Every company that processes data for HITOP Cloud, what it sees and where."
---
# Sub-processors

| Company | What for | What it sees | Where |
| --- | --- | --- | --- |
| The VM hosting provider (owner fills in the name) | the server HITOP Cloud runs on | everything the server stores: ciphertext and the metadata on the [security page](/security.html) | EU (owner fills in the data centre) |
| Lemon Squeezy (merchant of record) | payments, tax, receipts | buyers' names, addresses, payment details, the organization id | USA, with EU standard contractual clauses |
| Transactional mail service (to be chosen: Postmark or Amazon SES) | sign-in codes, invites, alerts | email addresses and the text of those mails | to be filled in when chosen |
| Backup storage (to be chosen) | encrypted backups | encrypted files only; the key stays offline | another provider in the EU |
| Uptime checker (to be chosen) | availability checks | nothing personal: it calls `/healthz` | to be filled in when chosen |

GitHub hosts the source code and receives no customer data. A new sub-processor is announced here and mailed to Business and Enterprise customers 30 days before it starts.
