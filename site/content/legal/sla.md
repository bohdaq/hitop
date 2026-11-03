---
path: "/legal/sla.html"
title: "Service Level Agreement · HITOP Cloud"
description: "HITOP Cloud's availability target for Business (99.5% a month, on one shared server) and the service credits."
---
# Service Level Agreement

**Applies to HITOP Cloud Business.** Free, Pro and Team have no availability target.

## Target

The sync API at `sync.hitop.rws8.tech` is available **99.5% of each calendar month** (about 3.6 hours of downtime). HITOP Cloud runs on a single server shared with other sites, which is why no higher figure is offered. A higher target needs a dedicated host or a self hosted setup under an Enterprise contract.

## How it is measured

An uptime check outside the server's provider calls `/healthz` every minute. A minute counts as down when the check fails from two locations. The public status page shows the history.

## Not counted

Maintenance announced 48 hours ahead (at most 4 hours a month), problems on the customer's side or the internet between, and suspension under the Terms.

## Credits

| Monthly availability | Credit on that month's fee |
| --- | --- |
| below 99.5% | 10% |
| below 99.0% | 25% |
| below 95.0% | 50% |

Ask by mail within 30 days after the month ends. Credits are taken off the next invoice and are the only remedy for missing the target.

## Support

Business: email, an answer within one business day (Monday to Friday, CET, public holidays excepted).
