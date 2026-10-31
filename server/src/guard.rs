//! What the proxy may reach: public unicast addresses on ports that aren't
//! well known for non-HTTP protocols. Checked on resolved addresses (see
//! `PublicResolver`), so a hostname pointing at an internal IP is refused too.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use reqwest::dns::{Addrs, Name, Resolve, Resolving};

/// The Fetch standard's "bad ports" (SMTP, IMAP, SSH and friends): a browser
/// refuses them, so the proxy does too. Stops the proxy relaying mail.
const BAD_PORTS: &[u16] = &[
    0, 1, 7, 9, 11, 13, 15, 17, 19, 20, 21, 22, 23, 25, 37, 42, 43, 53, 69, 77, 79, 87, 95, 101, 102, 103, 104, 109, 110,
    111, 113, 115, 117, 119, 123, 135, 137, 139, 143, 161, 179, 389, 427, 465, 512, 513, 514, 515, 526, 530, 531, 532,
    540, 548, 554, 556, 563, 587, 601, 636, 989, 990, 993, 995, 1719, 1720, 1723, 2049, 3659, 4045, 4190, 5060, 5061,
    6000, 6566, 6665, 6666, 6667, 6668, 6669, 6679, 6697, 10080,
];

pub fn port_allowed(port: u16) -> bool {
    !BAD_PORTS.contains(&port)
}

pub fn ip_allowed(ip: IpAddr, deny: &[IpAddr]) -> bool {
    if deny.contains(&ip) {
        return false;
    }
    match ip {
        IpAddr::V4(v4) => v4_public(v4),
        IpAddr::V6(v6) => v6_public(v6),
    }
}

fn v4_public(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(a == 0                                  // "this network"
        || a == 10                            // private
        || a == 127                           // loopback
        || (a == 100 && (64..128).contains(&b)) // CGNAT
        || (a == 169 && b == 254)             // link local, cloud metadata
        || (a == 172 && (16..32).contains(&b)) // private
        || (a == 192 && b == 0 && c == 0)     // IETF protocol assignments
        || (a == 192 && b == 0 && c == 2)     // documentation
        || (a == 192 && b == 88 && c == 99)   // 6to4 relay anycast
        || (a == 192 && b == 168)             // private
        || (a == 198 && (18..20).contains(&b)) // benchmarking
        || (a == 198 && b == 51 && c == 100)  // documentation
        || (a == 203 && b == 0 && c == 113)   // documentation
        || a >= 224) // multicast, reserved, broadcast
}

fn v6_public(ip: Ipv6Addr) -> bool {
    if let Some(v4) = ip.to_ipv4_mapped() {
        return v4_public(v4);
    }
    let s = ip.segments();
    // NAT64 (64:ff9b::/96) and 6to4 (2002::/16) carry an IPv4 address: judge that.
    if s[0] == 0x64 && s[1] == 0xff9b && s[2..6] == [0, 0, 0, 0] {
        return v4_public(Ipv4Addr::new((s[6] >> 8) as u8, s[6] as u8, (s[7] >> 8) as u8, s[7] as u8));
    }
    if s[0] == 0x2002 {
        return v4_public(Ipv4Addr::new((s[1] >> 8) as u8, s[1] as u8, (s[2] >> 8) as u8, s[2] as u8));
    }
    // Only global unicast (2000::/3), minus documentation (2001:db8::/32).
    (s[0] & 0xe000) == 0x2000 && !(s[0] == 0x2001 && s[1] == 0x0db8)
}

/// reqwest resolver that drops every non-public address. Resolving here (not
/// once up front) means each redirect hop and each new connection is checked,
/// and the address checked is the address connected to: no DNS rebinding gap.
pub struct PublicResolver {
    pub deny: Vec<IpAddr>,
}

impl Resolve for PublicResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_string();
        let deny = self.deny.clone();
        Box::pin(async move {
            let all: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), 0)).await?.collect();
            let allowed: Vec<SocketAddr> = all.into_iter().filter(|a| ip_allowed(a.ip(), &deny)).collect();
            if allowed.is_empty() {
                return Err(format!("{host} resolves only to private or reserved addresses").into());
            }
            Ok(Box::new(allowed.into_iter()) as Addrs)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(s: &str) -> bool {
        ip_allowed(s.parse().unwrap(), &[])
    }

    #[test]
    fn refuses_internal_addresses() {
        for ip in [
            "127.0.0.1", "10.1.0.32", "172.16.0.1", "192.168.1.1", "169.254.169.254", "100.64.0.1", "0.0.0.0",
            "255.255.255.255", "224.0.0.1", "::1", "::", "fe80::1", "fc00::1", "fd12::1", "::ffff:127.0.0.1",
            "::ffff:10.0.0.1", "64:ff9b::a00:1", "2002:7f00:1::", "2001:db8::1", "ff02::1",
        ] {
            assert!(!ok(ip), "{ip} should be refused");
        }
    }

    #[test]
    fn allows_public_addresses() {
        for ip in ["1.1.1.1", "8.8.8.8", "140.82.112.3", "2606:4700:4700::1111", "::ffff:1.1.1.1", "64:ff9b::808:808"] {
            assert!(ok(ip), "{ip} should be allowed");
        }
    }

    #[test]
    fn deny_list_wins() {
        let me: IpAddr = "168.119.17.57".parse().unwrap();
        assert!(!ip_allowed(me, &[me]));
    }

    #[test]
    fn ports() {
        assert!(port_allowed(443) && port_allowed(8080) && port_allowed(3000));
        assert!(!port_allowed(25) && !port_allowed(587) && !port_allowed(22) && !port_allowed(0));
    }
}
