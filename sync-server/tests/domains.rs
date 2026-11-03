//! Domain claims (HITOP Cloud C5, orgsso.rs): an org proves it owns `corp.example` with a DNS TXT
//! record, looked up over DNS over HTTPS (common/dns.rs plays the resolver). One org at a time per
//! domain, no free-mail domains, admins only, nothing without the resolver.

mod common;

use common::dns::MockDns;
use common::orgsso;
use common::*;
use serde_json::json;

#[test]
fn a_domain_is_verified_by_its_txt_record_for_one_org_at_a_time() {
    let dns = MockDns::start();
    let s = orgsso::server("domains", &dns, |_| {});
    let ada = Device::new(&s, "ada@corp.test");
    let eve = Device::new(&s, "eve@evil.test");
    let a = orgsso::org(&s, &ada, "Corp");
    let e = orgsso::org(&s, &eve, "Evil");

    let r = s.call(
        "POST",
        &format!("/v1/orgs/{a}/domains"),
        Some(&ada.session),
        Some(json!({ "domain": " Corp.Test. " })),
    );
    assert_eq!(r.status, 201, "{}", r.json);
    assert_eq!(
        (
            r.json["domain"].as_str(),
            r.json["record"].as_str(),
            r.json["verified"].as_bool()
        ),
        (
            Some("corp.test"),
            Some("_hitop-verify.corp.test"),
            Some(false)
        )
    );
    let value = r.json["value"].as_str().unwrap().to_string();
    assert!(
        value.starts_with("hitop-verify=") && value.len() == 13 + 32,
        "{value}"
    );
    assert_eq!(
        s.call(
            "POST",
            &format!("/v1/orgs/{a}/domains"),
            Some(&ada.session),
            Some(json!({ "domain": "corp.test" }))
        )
        .status,
        409
    );

    // No record yet, then a wrong one: not verified, and the answer says why.
    let verify = |org: &str, d: &Device| {
        s.call(
            "POST",
            &format!("/v1/orgs/{org}/domains/corp.test/verify"),
            Some(&d.session),
            None,
        )
    };
    let r = verify(&a, &ada);
    assert_eq!((r.status, r.json["verified"].as_bool()), (200, Some(false)));
    assert!(
        r.json["problem"]
            .as_str()
            .unwrap()
            .contains("no TXT record"),
        "{}",
        r.json
    );
    dns.set(
        "_hitop-verify.corp.test",
        &["v=spf1 -all", "hitop-verify=0000"],
    );
    assert!(verify(&a, &ada).json["problem"]
        .as_str()
        .unwrap()
        .contains("doesn't hold"));
    assert_eq!(
        dns.queries.lock().unwrap().last().map(String::as_str),
        Some("_hitop-verify.corp.test")
    );

    // Another org claims the same domain (claims are per org) but can't verify it while its
    // record names someone else's token.
    let r = s.call(
        "POST",
        &format!("/v1/orgs/{e}/domains"),
        Some(&eve.session),
        Some(json!({ "domain": "corp.test" })),
    );
    assert_eq!(r.status, 201);
    let eve_value = r.json["value"].as_str().unwrap().to_string();

    dns.set("_hitop-verify.corp.test", &["v=spf1 -all", &value]);
    let r = verify(&a, &ada);
    assert_eq!(
        (r.json["verified"].as_bool(), r.json["problem"].is_null()),
        (Some(true), true),
        "{}",
        r.json
    );
    // Eve gets control of the DNS later: the domain stays with the org that verified it first.
    dns.set("_hitop-verify.corp.test", &[&eve_value]);
    let r = verify(&e, &eve);
    assert_eq!(
        (r.status, r.json["error"].as_str()),
        (409, Some("domain_taken"))
    );
    // Corp removes it; then Evil's proof counts.
    assert_eq!(
        s.call(
            "DELETE",
            &format!("/v1/orgs/{a}/domains/corp.test"),
            Some(&ada.session),
            None
        )
        .status,
        204
    );
    assert_eq!(verify(&e, &eve).json["verified"], true);
    let listed = s
        .call(
            "GET",
            &format!("/v1/orgs/{e}/domains"),
            Some(&eve.session),
            None,
        )
        .json;
    assert_eq!(listed["domains"].as_array().unwrap().len(), 1);
    assert_eq!(
        s.call(
            "GET",
            &format!("/v1/orgs/{a}/domains"),
            Some(&ada.session),
            None
        )
        .json["domains"],
        json!([])
    );
}

#[test]
fn who_may_claim_what() {
    let dns = MockDns::start();
    let s = orgsso::server("domains-who", &dns, |_| {});
    let ada = Device::new(&s, "ada@corp.test");
    let bob = Device::new(&s, "bob@corp.test");
    let a = orgsso::org(&s, &ada, "Corp");
    assert_eq!(
        s.call(
            "POST",
            &format!("/v1/orgs/{a}/members"),
            Some(&ada.session),
            Some(json!({ "email": "bob@corp.test", "role": "member" }))
        )
        .status,
        201
    );
    // Members don't, strangers get 404.
    assert_eq!(
        s.call(
            "POST",
            &format!("/v1/orgs/{a}/domains"),
            Some(&bob.session),
            Some(json!({ "domain": "corp.test" }))
        )
        .status,
        403
    );
    let cy = Device::new(&s, "cy@else.test");
    assert_eq!(
        s.call(
            "GET",
            &format!("/v1/orgs/{a}/domains"),
            Some(&cy.session),
            None
        )
        .status,
        404
    );
    // Free mail and malformed names are refused.
    for d in ["gmail.com", "outlook.com", "corp", "a_b.test", "-x.test"] {
        assert_eq!(
            s.call(
                "POST",
                &format!("/v1/orgs/{a}/domains"),
                Some(&ada.session),
                Some(json!({ "domain": d }))
            )
            .status,
            400,
            "{d}"
        );
    }
    // At most 20.
    for i in 0..20 {
        assert_eq!(
            s.call(
                "POST",
                &format!("/v1/orgs/{a}/domains"),
                Some(&ada.session),
                Some(json!({ "domain": format!("d{i}.test") }))
            )
            .status,
            201
        );
    }
    assert_eq!(
        s.call(
            "POST",
            &format!("/v1/orgs/{a}/domains"),
            Some(&ada.session),
            Some(json!({ "domain": "d20.test" }))
        )
        .status,
        400
    );

    // Without a resolver nothing can be verified.
    let s = Server::with("domains-nodns", |c| c.host_key = Some(orgsso::host_key()));
    let ada = Device::new(&s, "ada@corp.test");
    let a = orgsso::org(&s, &ada, "Corp");
    assert_eq!(
        s.call(
            "POST",
            &format!("/v1/orgs/{a}/domains"),
            Some(&ada.session),
            Some(json!({ "domain": "corp.test" }))
        )
        .status,
        201
    );
    let r = s.call(
        "POST",
        &format!("/v1/orgs/{a}/domains/corp.test/verify"),
        Some(&ada.session),
        None,
    );
    assert_eq!((r.status, r.json["error"].as_str()), (503, Some("dns_off")));
    let sso = s
        .call(
            "GET",
            &format!("/v1/orgs/{a}/sso"),
            Some(&ada.session),
            None,
        )
        .json;
    assert_eq!(
        (sso["available"].as_bool(), sso["dnsVerification"].as_bool()),
        (Some(true), Some(false))
    );
}
