#![cfg(test)]

use super::{host_is_private, require_public, resolve_redirect, scheme_host};

#[test]
fn scheme_host_parse() {
    assert_eq!(
        scheme_host("https://w.wallhaven.cc/full/ab/x.jpg"),
        Some(("https".into(), "w.wallhaven.cc".into()))
    );
    assert_eq!(
        scheme_host("http://user:pass@10.0.0.5:8080/x"),
        Some(("http".into(), "10.0.0.5".into()))
    );
    assert_eq!(
        scheme_host("https://a:b@th.wallhaven.cc:443/x"),
        Some(("https".into(), "th.wallhaven.cc".into()))
    );
    assert_eq!(scheme_host("http://[::1]:9000/x"), Some(("http".into(), "::1".into())));
    assert_eq!(scheme_host("/relative/path"), None);
    assert_eq!(scheme_host("nonsense"), None);
    assert_eq!(scheme_host("HTTPS://Example.COM/x"), Some(("https".into(), "example.com".into())));
}

#[test]
fn private_host_literals() {
    for host in ["127.0.0.1", "10.1.2.3", "172.16.0.1", "192.168.1.1", "169.254.169.254", "0.0.0.0"]
    {
        assert!(host_is_private(host), "{host}");
    }
    for host in ["::1", "fc00::1", "fd12::1", "fe80::1", "::ffff:127.0.0.1"] {
        assert!(host_is_private(host), "{host}");
    }
    assert!(host_is_private("localhost"));
    assert!(host_is_private("db.localhost"));
    for host in ["1.1.1.1", "w.wallhaven.cc", "8.8.8.8", "2606:4700::1111"] {
        assert!(!host_is_private(host), "{host}");
    }
}

#[test]
fn public_blocks_ssrf() {
    for url in [
        "http://127.0.0.1/x",
        "http://localhost:9000/x",
        "http://169.254.169.254/latest/meta-data",
        "http://10.0.0.5/x",
        "http://[::1]/x",
        "file:///etc/passwd",
        "gopher://x/1",
        "ftp://example.com/x",
        "/etc/passwd",
    ] {
        assert!(require_public(url).is_err(), "{url}");
    }
    assert!(require_public("https://w.wallhaven.cc/full/ab/wallhaven-abc.jpg").is_ok());
    assert!(require_public("http://1.1.1.1/pic.jpg").is_ok());
}

#[test]
fn public_blocks_encoded_hosts() {
    for url in [
        "http://2130706433/x",
        "http://0x7f000001/x",
        "http://017700000001/x",
        "http://[::ffff:127.0.0.1]/x",
        "http://[::ffff:10.0.0.5]/x",
        "http://[fe80::1%25eth0]/x",
    ] {
        assert!(require_public(url).is_err(), "{url}");
    }
    assert_eq!(
        scheme_host("http://user@localhost@example.com/x"),
        Some(("http".into(), "example.com".into()))
    );
}

#[test]
fn errors_omit_url() {
    for url in ["http://user:secret@127.0.0.1/x?token=abc", "file:///etc/passwd", "not a url"] {
        let msg = require_public(url).unwrap_err();
        assert!(!msg.contains("secret"), "{msg}");
        assert!(!msg.contains("token"), "{msg}");
        assert!(!msg.contains("/etc/passwd"), "{msg}");
    }
}

#[test]
fn resolve_redirect_forms() {
    assert_eq!(
        resolve_redirect("https://a.cc/x/y", "https://b.cc/z").as_deref(),
        Some("https://b.cc/z")
    );
    assert_eq!(resolve_redirect("https://a.cc/x/y", "//b.cc/z").as_deref(), Some("https://b.cc/z"));
    assert_eq!(resolve_redirect("https://a.cc/x/y", "/z").as_deref(), Some("https://a.cc/z"));
    assert_eq!(
        resolve_redirect("https://a.cc/x/y", "z.jpg").as_deref(),
        Some("https://a.cc/x/z.jpg")
    );
    assert_eq!(
        resolve_redirect("https://a.cc/x/y?q=1", "?page=2").as_deref(),
        Some("https://a.cc/x/y?page=2")
    );
    assert_eq!(resolve_redirect("https://a.cc/x", ""), None);
    assert_eq!(resolve_redirect("nonsense", "/z"), None);
}
