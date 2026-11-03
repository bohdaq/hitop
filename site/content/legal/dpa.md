---
path: "/legal/dpa.html"
title: "Data Processing Agreement · HITOP Cloud"
description: "The data processing agreement for organizations on HITOP Cloud Business and Enterprise, under Article 28 GDPR."
---
# Data Processing Agreement

This agreement applies when an organization (the **customer**, controller) uses HITOP Cloud Business or Enterprise and Bohdan Tsap (the **processor**) processes personal data for it. It forms part of the [Terms of Service](/legal/terms.html).

## 1. Subject and duration

The processor hosts the customer's encrypted workspaces, accounts of its members and the related metadata for as long as the customer's subscription runs, plus the deletion period in section 8.

## 2. Nature of the data

Members' email addresses, device public keys, roles, security log entries (truncated IP addresses, user agents) and workspace ciphertext. Workspace content is end to end encrypted: the processor can't read it and has no key for it.

## 3. Instructions

The processor processes the data only to provide the service and as the customer instructs through the service's settings, unless EU or member state law requires otherwise (in which case it tells the customer first, unless that law forbids it).

## 4. Confidentiality and security

Only the operator has access, bound to confidentiality. Measures: end to end encryption of content, TLS in transit, sandboxed services with resource limits, a database on a local socket, encrypted off site backups, access reviews, a hash chained audit log. The [security page](/security.html) describes them and their limits, including the shared server.

## 5. Sub-processors

The customer authorizes the sub-processors on the [list](/legal/subprocessors.html). The processor announces a new one 30 days ahead by mail; the customer may object and, if no solution is found, end the subscription with a pro rata refund.

## 6. Assistance

The processor helps the customer answer data subject requests, and with security, breach notification and impact assessments, as far as the processor can given that it can't read the content.

## 7. Breaches

The processor tells the customer without undue delay and at the latest within 48 hours of becoming aware of a personal data breach, with what it knows.

## 8. Deletion and return

At the end of the subscription the customer can export with `hitop sync pull`. The processor deletes the organization's data within 30 days, and from backups when they expire (35 days).

## 9. Audits

The processor provides the information needed to show compliance (policies, access reviews, this agreement) and allows a reasonable audit once a year on 30 days' notice, at the customer's cost.

## 10. Transfers

Data is hosted in the EU. A transfer outside the EU by a sub-processor relies on an adequacy decision or the EU standard contractual clauses.
