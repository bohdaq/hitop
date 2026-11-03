# Vendor management policy

Status: draft for the owner's approval (C1, 2026-11-02). Review yearly.

1. The owner approves every vendor that stores, processes or carries customer data or runs part of
   the service, before it is used. The vendor list is [vendors.md](vendors.md).
2. For each vendor the list records: what it does for HITOP Cloud, what data it sees, where, and
   its own compliance report (SOC 2, ISO 27001) or why there is none.
3. Each vendor that processes personal data on HITOP's behalf is a sub-processor: it appears on
   the public sub-processor list (`/legal/subprocessors.html`, C3) and has a data processing
   agreement with the owner's company.
4. Vendors get the least data they need: the mail service gets addresses and the text of sign-in
   and invite mails; the backup bucket gets only age-encrypted dumps; Stripe gets billing
   details the buyer types into its checkout, never workspace data.
5. Yearly, and when a vendor changes its terms or has an incident, the owner re-checks the entry.
   A vendor that no longer meets the need is replaced and its data deleted; recorded in the ops
   log.
