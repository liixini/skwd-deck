use std::net::IpAddr;

use url::Url;

pub fn scheme_host(raw: &str) -> Option<(String, String)> {
    let parsed = Url::parse(raw.trim()).ok()?;
    let host = match parsed.host() {
        Some(url::Host::Domain(name)) => name.to_ascii_lowercase(),
        Some(url::Host::Ipv4(v4)) => v4.to_string(),
        Some(url::Host::Ipv6(v6)) => v6.to_string(),
        None => return None,
    };
    if host.is_empty() {
        return None;
    }
    Some((parsed.scheme().to_ascii_lowercase(), host))
}

fn ip_is_blocked(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.octets()[0] == 0
        }
        IpAddr::V6(v6) => {
            if v6.is_loopback() || v6.is_unspecified() {
                return true;
            }
            if let Some(v4) = v6.to_ipv4_mapped() {
                return ip_is_blocked(IpAddr::V4(v4));
            }
            (v6.octets()[0] & 0xfe) == 0xfc || (v6.segments()[0] & 0xffc0) == 0xfe80
        }
    }
}

pub fn host_is_private(host: &str) -> bool {
    if let Ok(ip) = host.parse::<IpAddr>() {
        return ip_is_blocked(ip);
    }
    let host = host.to_ascii_lowercase();
    host == "localhost" || host.ends_with(".localhost")
}

pub fn require_public(raw: &str) -> Result<(), String> {
    let Some((scheme, host)) = scheme_host(raw) else {
        return Err("blocked unparseable or relative URL".to_string());
    };
    if scheme != "http" && scheme != "https" {
        return Err(format!("blocked non-http(s) scheme: {scheme}"));
    }
    if host_is_private(&host) {
        return Err(format!("blocked private/loopback/link-local host: {host}"));
    }
    Ok(())
}

pub fn resolve_redirect(base: &str, location: &str) -> Option<String> {
    let location = location.trim();
    if location.is_empty() {
        return None;
    }
    let resolved = Url::parse(base).ok()?.join(location).ok()?;
    Some(resolved.into())
}

mod tests;
