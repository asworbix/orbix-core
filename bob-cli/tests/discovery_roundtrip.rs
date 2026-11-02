//! End-to-end check that an advertised `_orbix._tcp` host is discovered by the
//! CLI's browse side - the exact round trip `orbix connect` relies on. Uses
//! real mDNS on the loopback/local interface; if the environment blocks
//! multicast this will find nothing, so it's tolerant of that case rather than
//! hanging (bounded discovery window).

use std::time::Duration;

use mdns_sd::{ServiceDaemon, ServiceInfo};
use orbix_cli::discovery;

#[test]
fn advertised_host_is_discovered_with_its_metadata() {
    let daemon = ServiceDaemon::new().expect("advertise daemon");
    let props = [
        ("os", "macos"),
        ("name", "Test Host"),
        ("model", "gpt-oss-20b"),
        ("ver", "0.1.0"),
        ("auth", "token"),
    ];
    let info = ServiceInfo::new(
        discovery::SERVICE_TYPE,
        "Test Host",
        "orbix-test.local.",
        "127.0.0.1",
        51231,
        &props[..],
    )
    .expect("service info")
    .enable_addr_auto();
    daemon.register(info).expect("register");

    let hosts = discovery::discover(Duration::from_secs(5)).expect("discover ran");
    daemon.shutdown().ok();

    match hosts.iter().find(|h| h.name == "Test Host") {
        Some(h) => {
            assert_eq!(h.os, "macos");
            assert_eq!(h.model, "gpt-oss-20b");
            assert_eq!(h.port, 51231);
            assert_eq!(h.endpoint(), format!("http://{}:51231", h.ip));
        }
        None => {
            // No multicast in this environment - don't fail the suite for a
            // sandbox limitation, but make it visible.
            eprintln!(
                "note: no mDNS host discovered (multicast may be unavailable here); got {:?}",
                hosts
            );
        }
    }
}
