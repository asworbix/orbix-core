//! LAN peer discovery (advertise side).
//!
//! When the user turns the local endpoint on (Settings > Models > Expose
//! local model endpoint), this host advertises an mDNS service so other
//! devices on the network can find it with `orbix connect` instead of anyone
//! having to type an IP. The advertisement carries only metadata - device
//! name, OS, loaded model, app version, and the fact that auth is required.
//! It NEVER carries the token: the secret stays out of the broadcast and the
//! user pastes it (or pairs) on the connecting device.
//!
//! The code is deliberately platform-agnostic (mdns-sd is pure Rust), so the
//! Windows port advertises the same `_orbix._tcp` service and the two
//! directions - Mac hosting for Windows, Windows hosting for Mac - are
//! symmetric.

use std::sync::OnceLock;

use mdns_sd::{ServiceDaemon, ServiceInfo};

/// DNS-SD service type. The trailing dot is required by mdns-sd.
pub const SERVICE_TYPE: &str = "_orbix._tcp.local.";

/// The port the endpoint (mobile_api warp server) binds. Must match
/// mobile_api::start_mobile_api and endpoint::EndpointSettings::base_url.
pub const ENDPOINT_PORT: u16 = 51231;

/// One daemon for the process lifetime. Registration is toggled on/off as the
/// endpoint setting flips; the daemon itself keeps running (cheap, idle).
static DAEMON: OnceLock<ServiceDaemon> = OnceLock::new();

fn daemon() -> Option<&'static ServiceDaemon> {
    if let Some(d) = DAEMON.get() {
        return Some(d);
    }
    match ServiceDaemon::new() {
        Ok(d) => {
            // set() fails only if another thread won the race; either daemon
            // works, so fall back to whatever is now stored.
            let _ = DAEMON.set(d);
            DAEMON.get()
        }
        Err(e) => {
            eprintln!("[discovery] could not start mDNS daemon: {e}");
            None
        }
    }
}

/// Discover this machine's LAN IP via the UDP-socket trick: "connect" a UDP
/// socket to a public address (no packets are sent) and read back the local
/// address the OS picked. Returns None if there's no route out.
pub fn get_local_ip() -> Option<String> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    socket.local_addr().ok().map(|a| a.ip().to_string())
}

/// This machine's OS as a short tag other devices display in the host list.
fn os_tag() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

/// A human-friendly device name for the host list. Prefers the OS hostname;
/// falls back to a generic label. Kept dependency-free by shelling to the
/// platform's hostname source, which both macOS and Windows provide.
pub fn device_name() -> String {
    // COMPUTERNAME on Windows, HOSTNAME on many shells; then the `hostname`
    // command as a last resort. Strip any trailing ".local".
    let raw = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            std::process::Command::new("hostname")
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_else(|| "Orbix host".to_string());
    raw.trim_end_matches(".local").to_string()
}

/// A DNS-SD-safe instance name (the label shown in the service list). Keeps
/// letters, digits, spaces and hyphens; collapses everything else.
fn instance_label(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == ' ' || c == '-' { c } else { '-' })
        .collect();
    let trimmed = cleaned.trim().trim_matches('-').trim();
    if trimmed.is_empty() { "Orbix host".to_string() } else { trimmed.to_string() }
}

fn full_service_name(instance: &str) -> String {
    format!("{}.{}", instance, SERVICE_TYPE)
}

/// Start or stop advertising this host on the LAN. Call with the endpoint's
/// enabled state whenever it changes and once on startup. `model_id` is the
/// currently loaded model, shown so a connecting device knows what it will be
/// talking to; None or empty renders as "-".
pub fn set_advertising(enabled: bool, model_id: Option<&str>) {
    let Some(daemon) = daemon() else { return };

    let instance = instance_label(&device_name());
    let full = full_service_name(&instance);

    if !enabled {
        // Best-effort: unregister returns a receiver we don't need to await.
        let _ = daemon.unregister(&full);
        return;
    }

    let Some(ip) = get_local_ip() else {
        eprintln!("[discovery] no LAN IP; not advertising");
        return;
    };

    let model = model_id.map(|m| m.trim()).filter(|m| !m.is_empty()).unwrap_or("-");
    let properties: [(&str, &str); 5] = [
        ("os", os_tag()),
        ("name", instance.as_str()),
        ("model", model),
        ("ver", env!("CARGO_PKG_VERSION")),
        // Advertise that a token is required, but never the token itself.
        ("auth", "token"),
    ];

    // host_name must end in a dot; the address is our resolved LAN IP.
    let host_name = format!("{}.local.", instance.replace(' ', "-"));
    let service = match ServiceInfo::new(
        SERVICE_TYPE,
        &instance,
        &host_name,
        ip.as_str(),
        ENDPOINT_PORT,
        &properties[..],
    ) {
        Ok(s) => s.enable_addr_auto(),
        Err(e) => {
            eprintln!("[discovery] could not build service info: {e}");
            return;
        }
    };

    // Re-register is idempotent enough for our use: unregister first so a
    // model change refreshes the TXT record cleanly.
    let _ = daemon.unregister(&full);
    if let Err(e) = daemon.register(service) {
        eprintln!("[discovery] could not register mDNS service: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instance_label_is_dns_safe() {
        assert_eq!(instance_label("Anna's MacBook"), "Anna-s MacBook");
        assert_eq!(instance_label("  weird//name  "), "weird--name");
        assert_eq!(instance_label(""), "Orbix host");
        assert_eq!(instance_label("---"), "Orbix host");
    }

    #[test]
    fn device_name_is_never_empty() {
        assert!(!device_name().is_empty());
    }

    #[test]
    fn os_tag_is_one_of_the_known_set() {
        assert!(["macos", "windows", "linux"].contains(&os_tag()));
    }

    #[test]
    fn service_type_and_port_match_the_endpoint() {
        assert!(SERVICE_TYPE.starts_with("_orbix._tcp"));
        assert_eq!(ENDPOINT_PORT, 51231);
    }
}
