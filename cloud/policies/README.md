# HITOP Cloud policies (SOC 2 groundwork)

Written for HITOP Cloud C1 (spec/CLOUD_SERVICE.md section 7, owner decision Q17). Each policy
describes what the one operator (the owner) and the agents working for him actually do today, so
an auditor can test it. They are drafts until the owner approves them (section 11, row C1); the
status line in each file says so.

| Policy | File | Covers (SOC 2 trust services criteria, roughly) |
| --- | --- | --- |
| Information security | [information-security.md](information-security.md) | CC1, CC2, CC5: scope, roles, the encryption design, review cycle |
| Access control | [access-control.md](access-control.md) | CC6.1–CC6.3: who has access to what, how it is granted and removed, quarterly reviews |
| Change management | [change-management.md](change-management.md) | CC8.1: signed-off commits on `main`, CI green, deploys recorded |
| Incident response | [incident-response.md](incident-response.md) | CC7.3–CC7.5: detect, contain, tell customers, learn |
| Backup and recovery | [backup-recovery.md](backup-recovery.md) | A1.2, A1.3: backups, restore drills |
| Vendor management | [vendor-management.md](vendor-management.md) and [vendors.md](vendors.md) | CC9.2: vendors, what data they see, their reports |
| Data retention and deletion | [data-retention.md](data-retention.md) | C1.1, C1.2, P4: what is kept, how long, deletion on request |
| Acceptable use | [acceptable-use.md](acceptable-use.md) | CC1.1, CC2.3: what customers and operators may not do |

Evidence lives next to them: access reviews in [../evidence/access-reviews/](../evidence/access-reviews/),
restore drills in `../evidence/restore-drills/` (from C2), deploys and operator actions in
[../ops-log.md](../ops-log.md). The owner picks the compliance tool (Vanta, Drata, Secureframe or
a spreadsheet) and the auditor; these files load into whichever he picks.

Review: every policy once a year and whenever what it describes changes (a new vendor, a second
operator, a new host). A change to a policy is a commit like any other (change management).
