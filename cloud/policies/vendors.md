# Vendor list

Status: draft, 2026-11-02 (C1). The owner confirms each row (names marked "owner picks" aren't
chosen yet, spec/CLOUD_SERVICE.md section 11). Compliance reports are what each vendor publishes;
the owner downloads them into the compliance tool when he picks one.

| Vendor | Used for | Data it sees | Where | Compliance report | State |
| --- | --- | --- | --- | --- | --- |
| VM hosting provider of `bohdan-prod-donotdelete` (168.119.17.57; the owner confirms the provider and contract) | the VM: sync server, portal, site, database, local backups | everything on the VM: ciphertext, account metadata, logs | EU (owner confirms the data centre) | owner adds the provider's ISO 27001 certificate or report | in use |
| Backup object storage at another provider or region (owner picks) | off-site encrypted backups (C2) | age-encrypted dumps only | EU | owner adds | not yet chosen |
| Stripe (Stripe Managed Payments, with Link) | merchant of record: checkout, subscriptions, tax (VAT, sales tax), receipts and invoices, fraud, disputes, order support (C3; replaced Lemon Squeezy 2026-11-03) | buyer name, email, billing address, payment details (never seen by HITOP), the org and user ids in the Checkout Session and subscription metadata | EU (Stripe Technology Europe, Ireland) and US (Stripe's terms) | Stripe publishes PCI DSS Level 1 and SOC 1/SOC 2 reports (owner downloads them from the Dashboard) | sandbox account set up by the owner; live not yet |
| Transactional mail service: Postmark or Amazon SES (owner picks) | sign-in codes, invites, alerts (before the paid launch) | recipient addresses, mail text (codes, links, org names) | owner picks the region | Postmark: SOC 2 Type 2 reports on request; AWS: SOC 2 | not yet chosen; today mail goes through the VM's own mail server (`127.0.0.1:587`) |
| Uptime checker outside the VM's provider (owner picks) | off-host health checks of `/healthz` (C2) | the public URLs and their status | any | owner adds | not yet chosen |
| GitHub (Microsoft) | source code, CI (Actions), releases, issues | source code (public), CI secrets (crates.io token), no customer data | US | GitHub SOC 2 Type 2 | in use |
| DNS provider of `rws8.tech` (owner confirms) | DNS for `hitop.rws8.tech`, `sync.`, `cloud.`, `status.` | the names | owner confirms | owner adds | in use |
| crates.io, ghcr.io | publishing the CLI crates and container images | released code only | US | GitHub SOC 2 (ghcr.io); crates.io none (open source registry, no customer data) | in use |
| Apple (App Store Connect) | the Mac and iOS apps | app binaries, App Store buyer data stays with Apple | US | Apple's own | in use, not part of HITOP Cloud billing |
| Compliance tool: Vanta, Drata, Secureframe or a spreadsheet (owner picks) | SOC 2 evidence | policies, access lists, evidence files | owner picks | the tool's own SOC 2 | not yet chosen |
