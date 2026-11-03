---
path: "/legal/enterprise-agreement.html"
title: "Enterprise agreement (template) · HITOP Cloud"
description: "The template of HITOP's Enterprise agreement: support, upgrade windows and indemnity for HITOP Cloud Enterprise or a self-hosted HITOP sync server."
---

# Enterprise agreement (template)

This is the template the Enterprise agreement starts from; each customer's order form fills in the
blanks marked `[...]`. It is not an offer. The software stays open source: the agreement buys
support, upgrade windows and indemnity, not a different licence.

## 1. Parties and scope

1.1 This agreement is between `[HITOP's legal entity, address, company number]` ("HITOP") and
`[customer legal name, address]` ("Customer").

1.2 It covers one of (the order form says which):

- **Hosted**: HITOP Cloud Enterprise for the organizations named in the order form, on HITOP's
  infrastructure in the EU; or
- **Self-hosted**: the HITOP sync server run by Customer on Customer's infrastructure, from the
  signed images or source HITOP publishes.

1.3 The HITOP clients are licensed under the MIT licence and the sync server under the GNU Affero
General Public License v3.0 or later. Nothing in this agreement changes those licences or grants a
different one. Customer's obligations under the AGPL (offering the source of a modified server to
its users) stay Customer's.

## 2. Fees and term

2.1 Fees: `[per seat per year / flat fee]`, invoiced `[annually in advance]` by HITOP's company
outside the online store, payable within `[30]` days.

2.2 Term: `[12]` months from `[start date]`, renewing for further `[12]` months unless either party
gives `[60]` days' written notice.

## 3. Support

3.1 Channel: the support address in the order form; `[optional: a shared chat channel]`. Customer
names up to `[3]` contacts.

3.2 Response times (first answer, not resolution):

| Severity | Meaning | Hours | First answer |
| --- | --- | --- | --- |
| 1 | Service down or data at risk | `[24×7 / business hours]` | `[4 hours]` |
| 2 | Degraded, or a security issue without known exploitation | business hours `[Mon–Fri, CET]` | `[1 business day]` |
| 3 | Questions, minor bugs | business hours | `[2 business days]` |

3.3 Self-hosted: support covers the server as published by HITOP (unmodified, on a version still
supported under section 4), its documented settings, upgrades and recovery. Customer sends a
support bundle (`hitop-sync-server support-bundle`), which by design holds no customer data or
secrets; HITOP never asks for passwords, recovery phrases, tokens or database dumps with personal
data.

3.4 Hosted: availability and service credits as in the SLA attached as Annex A
(`[99.5% on the shared host / the figure agreed for a dedicated host]`).

## 4. Upgrades and versions

4.1 HITOP publishes upgrade notes with every release (database changes, settings, how to go
back) and signs release images.

4.2 Supported versions: the latest release and the `[two]` before it, and any release for
`[6]` months after its successor.

4.3 Hosted upgrade windows: `[Tuesdays 06:00–08:00 UTC]`, announced `[5]` business days ahead on
the status page and to Customer's contacts; security fixes may be applied at any time with notice
as soon as practical.

4.4 Self-hosted: HITOP tells Customer's contacts of a security release before publishing details,
where possible `[7]` days ahead.

## 5. Security and data protection

5.1 Hosted: the data processing agreement at `/legal/dpa.html` applies, with the sub-processors
listed at `/legal/subprocessors.html`. Data stays in the EU unless the order form names another
region.

5.2 HITOP keeps the security measures described at `/security.html` and the policies in its
repository (`cloud/policies/`), and answers Customer's reasonable security questionnaires once a
year.

5.3 Incidents affecting Customer's data are reported to Customer's contacts within `[72]` hours of
HITOP becoming aware, as in the incident response policy.

## 6. Indemnity

6.1 HITOP will defend Customer against a third-party claim that the HITOP software, as provided by
HITOP and used under this agreement, infringes that third party's intellectual property rights,
and pay the damages and costs finally awarded or agreed in a settlement HITOP approves.

6.2 Conditions: Customer tells HITOP promptly in writing, lets HITOP control the defence and
settlement, and cooperates at HITOP's cost.

6.3 Not covered: modifications not made by HITOP, combination with things HITOP didn't provide,
use after HITOP offered a non-infringing update, and third-party components under their own
licences (listed in each release's SBOM).

6.4 Remedies: HITOP may modify the software to be non-infringing, obtain a right to keep using
it, or end the agreement and refund prepaid fees for the remaining term.

## 7. Liability

7.1 Each party's total liability under this agreement is limited to the fees paid in the
`[12]` months before the claim, except for the indemnity in section 6 `[capped at: ...]`, breach of
confidentiality, and liability that cannot be limited by law.

7.2 Neither party is liable for indirect or consequential loss, lost profits or lost data, except
where caused by its gross negligence or wilful misconduct. Because collections are end-to-end
encrypted and HITOP holds no key, HITOP cannot recover a workspace whose keys Customer lost.

## 8. Confidentiality, termination, law

8.1 Each party keeps the other's confidential information confidential for `[3]` years after the
agreement ends.

8.2 Either party may terminate for material breach not cured within `[30]` days of notice. On the
end of a hosted agreement, Customer may export its workspaces (they are on its members' devices
and repositories in any case) for `[30]` days, after which HITOP deletes them.

8.3 Governing law `[...]`, courts of `[...]`.

## Annexes

- A: Service level (hosted).
- B: Order form (organizations or installation, seats, fees, contacts, region, chosen options).
- C: Data processing agreement (hosted).
