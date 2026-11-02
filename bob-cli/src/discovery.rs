//! LAN peer discovery (browse side).
//!
//! `orbix connect` uses this to find Orbix hosts advertising `_orbix._tcp` on
//! the network - the mirror of the app's advertise side. We browse for a
//! short window, collect the responders, and hand back a de-duplicated list
//! the CLI renders as a numbered menu. No token is ever on the wire; the host
//! only advertises metadata.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use mdns_sd::{ServiceDaemon, ServiceEvent};

pub const SERVICE_TYPE: &str = "_orbix._tcp.local.";

/// One discovered host on the LAN.
#[derive(Debug, Clone)]
pub struct Host {
    pub name: String,
    pub os: String,
    pub model: String,
    pub ip: String,
    pub port: u16,
}

impl Host {
    /// The base URL `orbix chat` connects to.
    pub fn endpoint(&self) -> String {
        format!("http://{}:{}", self.ip, self.port)
    }
}

/// Browse the LAN for Orbix hosts for `window`, returning what answered.
/// Blocking: run it off the async runtime (spawn_blocking). Hosts are keyed by
/// ip:port so a device answering on several interfaces appears once.
pub fn discover(window: Duration) -> Result<Vec<Host>, String> {
    let daemon = ServiceDaemon::new().map_err(|e| format!("mDNS start failed: {e}"))?;
    let recv = daemon
        .browse(SERVICE_TYPE)
        .map_err(|e| format!("mDNS browse failed: {e}"))?;

    let mut found: BTreeMap<String, Host> = BTreeMap::new();
    let deadline = Instant::now() + window;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match recv.recv_timeout(remaining) {
            Ok(ServiceEvent::ServiceResolved(info)) => {
                // LAN chat is IPv4; take the first advertised v4 address.
                let Some(addr) = info.get_addresses_v4().into_iter().next() else { continue };
                let port = info.get_port();
                let key = format!("{addr}:{port}");
                let prop = |k: &str| info.get_property_val_str(k).unwrap_or("").to_string();
                let name = {
                    let n = prop("name");
                    if n.is_empty() {
                        info.get_fullname()
                            .split('.')
                            .next()
                            .unwrap_or("Orbix host")
                            .to_string()
                    } else {
                        n
                    }
                };
                found.entry(key).or_insert(Host {
                    name,
                    os: prop("os"),
                    model: prop("model"),
                    ip: addr.to_string(),
                    port,
                });
            }
            Ok(_) => {}
            Err(_) => break, // timeout or channel closed
        }
    }
    let _ = daemon.shutdown();

    let mut hosts: Vec<Host> = found.into_values().collect();
    // Stable, friendly order: by display name then address.
    hosts.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then(a.ip.cmp(&b.ip)));
    Ok(hosts)
}
