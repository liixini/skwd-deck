#![cfg(test)]

use super::*;

fn info(name: &str) -> OutputInfo {
    OutputInfo {
        name: name.to_string(),
        make: String::new(),
        model: String::new(),
        width: 1920,
        height: 1080,
        refresh_mhz: 60000,
        scale: 1,
        rotated: false,
        logical: None,
        description: String::new(),
        serial: String::new(),
    }
}

#[test]
fn rotated_swaps_dims() {
    let flat = info("DP-1");
    assert_eq!(flat.logical_size(), (1920, 1080));
    let portrait = OutputInfo { rotated: true, ..info("DP-2") };
    assert_eq!(portrait.logical_size(), (1080, 1920));
}

#[test]
fn scaled_logical_size() {
    for (scale, rotated, expected) in [
        (2, false, (600, 800)),
        (2, true, (800, 600)),
        (1, true, (1600, 1200)),
        (0, false, (1200, 1600)),
        (-1, true, (1600, 1200)),
    ] {
        let output = OutputInfo { width: 1200, height: 1600, scale, rotated, ..info("DP-1") };
        assert_eq!(output.logical_size(), expected);
        assert_eq!(output.pixel_size(), if rotated { (1600, 1200) } else { (1200, 1600) });
        assert_eq!((output.width, output.height), (1200, 1600));
    }
}

#[test]
fn reported_logical_size() {
    for (rotated, logical) in [(false, (1280, 720)), (true, (720, 1280))] {
        let output = OutputInfo { scale: 2, rotated, logical: Some(logical), ..info("DP-1") };
        assert_eq!(output.logical_size(), logical);
        assert_eq!(output.pixel_size(), if rotated { (1080, 1920) } else { (1920, 1080) });
    }
    let invalid = OutputInfo { scale: 2, logical: Some((0, 720)), ..info("DP-1") };
    assert_eq!(invalid.logical_size(), (960, 540));
}

#[test]
fn refresh_caps_limit() {
    assert_eq!(effective_fps(165, 164_835), 165);
    assert_eq!(effective_fps(165, 59_997), 60);
    assert_eq!(effective_fps(165, 144_001), 144);
    assert_eq!(effective_fps(30, 164_835), 30);
    assert_eq!(effective_fps(165, 0), 165);
}

#[test]
fn target_cap_fastest() {
    let outputs = vec![
        OutputInfo { refresh_mhz: 164_835, ..info("DP-1") },
        OutputInfo { refresh_mhz: 59_997, ..info("DP-2") },
        OutputInfo { refresh_mhz: 144_001, ..info("DP-3") },
    ];
    assert_eq!(target_fps(165, "DP-2", &outputs), 60);
    assert_eq!(target_fps(165, "DP-2,DP-3", &outputs), 144);
    assert_eq!(target_fps(165, "*", &outputs), 165);
    assert_eq!(target_fps(30, "*", &outputs), 30);
    assert_eq!(target_fps(165, "missing", &outputs), 165);
    assert_eq!(refresh_signature(&outputs), "DP-1:164835,DP-2:59997,DP-3:144001");
    assert_eq!(fps_map(165, &outputs), "DP-1=165;DP-2=60;DP-3=144");
}

#[test]
fn fake_output_spec_parsing() {
    let sized = parse_fake_output("DP-1:2560x1440").unwrap();
    assert_eq!((sized.name.as_str(), sized.width, sized.height), ("DP-1", 2560, 1440));
    let defaulted = parse_fake_output("HDMI-A-1").unwrap();
    assert_eq!(
        (defaulted.name.as_str(), defaulted.width, defaulted.height),
        ("HDMI-A-1", 1920, 1080)
    );
    assert_eq!(parse_fake_output("  eDP-1 : 800 x 600 ").unwrap().name, "eDP-1");
    assert!(parse_fake_output("").is_none());
    assert!(parse_fake_output("   ").is_none());
}

#[test]
fn enumerate_cache_rules() {
    let _guard = enum_exclusive();
    invalidate();

    assert!(enumerate_with(Vec::new).is_empty());

    let mut fetched = false;
    let first = enumerate_with(|| {
        fetched = true;
        vec![info("DP-1")]
    });
    assert!(fetched);
    assert_eq!(first[0].name, "DP-1");

    let mut refetched = false;
    let cached = enumerate_with(|| {
        refetched = true;
        vec![info("DP-2")]
    });
    assert!(!refetched);
    assert_eq!(cached[0].name, "DP-1");

    invalidate();
    let mut after_invalidate = false;
    let fresh = enumerate_with(|| {
        after_invalidate = true;
        vec![info("DP-2")]
    });
    assert!(after_invalidate);
    assert_eq!(fresh[0].name, "DP-2");

    invalidate();
}

#[test]
fn identity_prefers_monitor() {
    let named =
        OutputInfo { make: String::from("Dell"), model: String::from("U2723QE"), ..info("DP-1") };
    assert_eq!(named.identity(), "Dell U2723QE");
    assert_eq!(info("DP-3").identity(), "DP-3");
    let model_only = OutputInfo { model: String::from("XG27AQ"), ..info("DP-2") };
    assert_eq!(model_only.identity(), "XG27AQ");
}

#[test]
fn portrait_from_rotation() {
    assert!(!info("DP-1").portrait());
    assert!(OutputInfo { rotated: true, ..info("DP-2") }.portrait());
}

#[test]
fn identity_key_pairs_connector() {
    let named =
        OutputInfo { make: String::from("Dell"), model: String::from("U2723QE"), ..info("DP-1") };
    assert_eq!(named.identity_key(), "Dell U2723QE @ DP-1");
    let twin =
        OutputInfo { make: String::from("Dell"), model: String::from("U2723QE"), ..info("DP-2") };
    assert_ne!(named.identity_key(), twin.identity_key());
    assert_eq!(named.identity(), twin.identity());
    assert_eq!(info("DP-3").identity_key(), "DP-3");
}

fn short_session() -> (Connection, std::thread::JoinHandle<()>) {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;

    let (client, mut server) = UnixStream::pair().unwrap();
    let thread = std::thread::spawn(move || {
        loop {
            let mut header = [0; 8];
            if server.read_exact(&mut header).is_err() {
                return;
            }
            let size = (u32::from_ne_bytes(header[4..].try_into().unwrap()) >> 16) as usize;
            let mut payload = vec![0; size - header.len()];
            server.read_exact(&mut payload).unwrap();
            if u32::from_ne_bytes(header[..4].try_into().unwrap()) != 1
                || u32::from_ne_bytes(header[4..].try_into().unwrap()) & 0xffff != 0
            {
                continue;
            }
            let callback = u32::from_ne_bytes(payload[..4].try_into().unwrap());
            let mut event = Vec::new();
            event.extend_from_slice(&callback.to_ne_bytes());
            event.extend_from_slice(&(12u32 << 16).to_ne_bytes());
            event.extend_from_slice(&0u32.to_ne_bytes());
            server.write_all(&event).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(100));
            return;
        }
    });
    (Connection::from_socket(client).unwrap(), thread)
}

#[test]
fn startup_connection_recovers() {
    let mut attempts = 0;
    let mut retries = 0;
    let mut changes = 0;
    let mut server = None;
    monitor_connections(
        || {
            attempts += 1;
            if attempts == 1 {
                return None;
            }
            let (conn, thread) = short_session();
            server = Some(thread);
            Some(conn)
        },
        || {
            retries += 1;
            retries < 2
        },
        || changes += 1,
    );
    server.unwrap().join().unwrap();
    assert_eq!(attempts, 2);
    assert_eq!(changes, 1);
}

#[test]
fn startup_handshake_recovers() {
    use std::os::unix::net::UnixStream;

    let mut attempts = 0;
    let mut retries = 0;
    let mut changes = 0;
    let mut server = None;
    monitor_connections(
        || {
            attempts += 1;
            if attempts == 1 {
                let (client, peer) = UnixStream::pair().unwrap();
                drop(peer);
                return Some(Connection::from_socket(client).unwrap());
            }
            let (conn, thread) = short_session();
            server = Some(thread);
            Some(conn)
        },
        || {
            retries += 1;
            retries < 2
        },
        || changes += 1,
    );
    server.unwrap().join().unwrap();
    assert_eq!(attempts, 2);
    assert_eq!(changes, 1);
}

#[test]
fn disconnected_session_recovers() {
    let mut attempts = 0;
    let mut retries = 0;
    let mut changes = 0;
    let mut servers = Vec::new();
    monitor_connections(
        || {
            attempts += 1;
            let (conn, thread) = short_session();
            servers.push(thread);
            Some(conn)
        },
        || {
            retries += 1;
            retries < 2
        },
        || changes += 1,
    );
    for server in servers {
        server.join().unwrap();
    }
    assert_eq!(attempts, 2);
    assert_eq!(changes, 1);
}

#[test]
fn monitor_key_follows_serial() {
    let dell = |description: &str| OutputInfo {
        make: "Dell Inc.".to_string(),
        model: "DELL P2723DE".to_string(),
        description: description.to_string(),
        ..info("DP-7")
    };
    assert_eq!(
        dell("Dell Inc. DELL P2723DE 317ZNR3 (DP-7)").monitor(),
        "Dell Inc. DELL P2723DE 317ZNR3"
    );
    assert_eq!(dell("Dell Inc. DELL P2723DE 317ZNR3").monitor(), "Dell Inc. DELL P2723DE 317ZNR3");
    assert_eq!(dell("Dell Inc. DELL P2723DE (DP-7)").monitor(), "Dell Inc. DELL P2723DE @ DP-7");
    assert_eq!(dell("").monitor(), "Dell Inc. DELL P2723DE @ DP-7");
    assert_eq!(dell("Built-in display").monitor(), "Dell Inc. DELL P2723DE @ DP-7");
    assert_eq!(info("eDP-1").monitor(), "eDP-1");
}

#[test]
fn monitor_key_prefers_head_serial() {
    let aoc = |name: &str, serial: &str| OutputInfo {
        make: "PNP(AOC)".to_string(),
        model: "2590G4".to_string(),
        description: format!("PNP(AOC) - 2590G4 - {name}"),
        serial: serial.to_string(),
        ..info(name)
    };
    assert_eq!(aoc("DP-9", "0x00002AB6").monitor(), "PNP(AOC) 2590G4 0x00002AB6");
    assert_eq!(aoc("HDMI-A-1", "0x00002AB6").monitor(), aoc("DP-9", "0x00002AB6").monitor());
    assert_eq!(aoc("DP-9", "").monitor(), "PNP(AOC) 2590G4 @ DP-9");
    assert_eq!(OutputInfo { serial: "0x1".to_string(), ..info("eDP-1") }.monitor(), "eDP-1");
}

#[test]
fn unclear_serials_are_dropped() {
    let panel = |name: &str, model: &str, serial: &str| OutputInfo {
        make: "AOC".to_string(),
        model: model.to_string(),
        serial: serial.to_string(),
        ..info(name)
    };
    let mut outputs = [
        panel("DP-1", "24G2", "123456789"),
        panel("DP-2", "24G2", "123456789"),
        panel("DP-3", "27G2", "123456789"),
        panel("DP-4", "27G2", "0x00000000"),
        panel("DP-5", "Q27", " 0x00002AB6 "),
    ];
    unique_serials(&mut outputs);
    let keys = outputs.iter().map(OutputInfo::monitor).collect::<Vec<_>>();
    assert_eq!(
        keys,
        [
            "AOC 24G2 @ DP-1",
            "AOC 24G2 @ DP-2",
            "AOC 27G2 123456789",
            "AOC 27G2 @ DP-4",
            "AOC Q27 0x00002AB6",
        ]
    );
}
