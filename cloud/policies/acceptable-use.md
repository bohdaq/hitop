# Acceptable use policy

Status: draft for the owner's approval (C1, 2026-11-02). The customer-facing version goes into the
Terms of Service (C3, lawyer review).

## Customers may not

- use HITOP Cloud to store or distribute malware, or content that is illegal where they or HITOP
  operate;
- try to reach other customers' data, probe or attack the service, or get around plan limits and
  rate limits (several free orgs to avoid paying for a team is a breach);
- send invites or mails to people who didn't ask for them (spam);
- resell the hosted service without a contract.

HITOP can't read workspace contents, so enforcement relies on metadata (volume, rates, reports).
Reports go to the abuse address on the site. Breaches lead to a warning, then suspension of the
org; data stays readable for export for 30 days unless the law requires otherwise.

## Operators (the owner and agents) must

- use production access only to run the service, never to look at a customer's metadata beyond
  what an incident or a support request needs;
- never copy the production database off the VM except as an encrypted backup;
- keep secrets in the places the information security policy names;
- follow `ROADMAP.md` section 3 on the VM (nothing outside HITOP's own files and the listed
  configuration).
