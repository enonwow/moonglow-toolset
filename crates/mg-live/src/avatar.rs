//! Avatar URLs are set by one principal and fetched by every DM's Moonglow, so
//! they must never point into a viewer's own network.

use std::net::IpAddr;

/// A public HTTPS URL with a DNS name: no credentials, IP literals or local names.
pub fn avatar_url(text: &str) -> Option<url::Url> {
    if text.len() > 2048 || text.chars().any(char::is_whitespace) {
        return None;
    }
    let url = url::Url::parse(text).ok()?;
    let host = match url.host()? {
        url::Host::Domain(d) => d.trim_end_matches('.').to_ascii_lowercase(),
        _ => return None,
    };
    let local = !host.contains('.')
        || ["localhost", "local", "internal", "lan", "home.arpa", "intranet", "corp"]
            .iter()
            .any(|s| host == *s || host.ends_with(&format!(".{s}")));
    (url.scheme() == "https" && url.username().is_empty() && url.password().is_none() && !local)
        .then_some(url)
}

/// The gateway route that serves an actor's avatar image (`GET`, session bearer). Clients load
/// avatars only from there, never from the advertised third-party URL.
pub fn avatar_path(id: &str) -> String {
    let mut base = url::Url::parse("https://gateway.invalid/avatars/").expect("static URL");
    base.path_segments_mut().expect("hierarchical URL").pop_if_empty().push(id);
    base.path().to_owned()
}

/// Whether a resolved address is a public unicast one (checked again at connect time).
pub fn public_address(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_multicast()
                || v4.is_documentation()
                || a == 0
                || a >= 240
                || (a == 192 && b == 0 && v4.octets()[2] == 0)
                || (a == 100 && (64..128).contains(&b))
                || (a == 198 && (b == 18 || b == 19)))
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return public_address(IpAddr::V4(v4));
            }
            let seg = v6.segments();
            match seg[0] {
                // 6to4 embeds an IPv4 address: check that one.
                0x2002 => public_address(IpAddr::V4(std::net::Ipv4Addr::new(
                    (seg[1] >> 8) as u8,
                    seg[1] as u8,
                    (seg[2] >> 8) as u8,
                    seg[2] as u8,
                ))),
                // Teredo (2001::/32) and documentation (2001:db8::/32).
                0x2001 if seg[1] == 0 || seg[1] == 0xdb8 => false,
                // Deny by default: only global unicast 2000::/3 (NAT64, IPv4-compatible,
                // site-local, ULA and link-local all fall outside it).
                first => (0x2000..0x4000).contains(&first),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_public_dns_https_urls_are_avatars() {
        assert!(avatar_url("https://cdn.example.org/a.png").is_some());
        for bad in [
            "http://cdn.example.org/a.png",
            "https://user:pw@cdn.example.org/a.png",
            "https://203.0.113.10/a.png",
            "https://[::1]/a.png",
            "https://127.0.0.1/a.png",
            "https://localhost/a.png",
            "https://printer/a.png",
            "https://nas.local/a.png",
            "https://wiki.corp/a.png",
            "https://router.home.arpa/a.png",
            "https://svc.internal./a.png",
        ] {
            assert!(avatar_url(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn avatar_paths_are_one_encoded_segment() {
        assert_eq!(avatar_path("ai:scribe-1"), "/avatars/ai:scribe-1");
        assert_eq!(avatar_path("ai:a/b?c#d e"), "/avatars/ai:a%2Fb%3Fc%23d%20e");
    }

    #[test]
    fn private_and_special_addresses_are_refused() {
        use std::net::{Ipv4Addr, Ipv6Addr};
        // Built from octets so the private-address leak rule stays strict.
        let v4 = |a, b, c, d| IpAddr::V4(Ipv4Addr::new(a, b, c, d));
        let mapped = Ipv4Addr::new(192, 168, 0, 1).to_ipv6_mapped();
        for bad in [
            v4(10, 0, 0, 1),
            v4(172, 16, 0, 1),
            v4(192, 168, 0, 1),
            v4(127, 0, 0, 1),
            v4(169, 254, 1, 1),
            v4(100, 64, 0, 1),
            v4(0, 0, 0, 0),
            IpAddr::V6(Ipv6Addr::LOCALHOST),
            IpAddr::V6(Ipv6Addr::new(0xfc00, 0, 0, 0, 0, 0, 0, 1)),
            IpAddr::V6(Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, 1)),
            IpAddr::V6(mapped),
            v4(240, 0, 0, 1),
            v4(192, 0, 0, 8),
            IpAddr::V6(Ipv6Addr::new(0x64, 0xff9b, 0, 0, 0, 0, 0xc0a8, 1)),
            IpAddr::V6(Ipv6Addr::new(0x2002, 0xc0a8, 0x0001, 0, 0, 0, 0, 1)),
            IpAddr::V6(Ipv6Addr::new(0x2001, 0, 0x4136, 0xe378, 0, 0, 0, 1)),
            IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1)),
            IpAddr::V6(Ipv6Addr::new(0xfec0, 0, 0, 0, 0, 0, 0, 1)),
            IpAddr::V6(Ipv4Addr::new(10, 0, 0, 1).to_ipv6_compatible()),
        ] {
            assert!(!public_address(bad), "{bad}");
        }
        assert!(public_address("93.184.216.34".parse().unwrap()));
        assert!(public_address("2606:4700::1111".parse().unwrap()));
    }
}
