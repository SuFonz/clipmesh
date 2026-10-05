//! Service discovery over mDNS.
//!
//! mDNS only ever answers one question: *which devices are on this network, and
//! where do I reach them?* It never carries clipboard content, and nothing it
//! says is trusted - the TXT record is unauthenticated by construction, so the
//! fingerprint it advertises is treated as a hint that must be confirmed during
//! the TLS handshake ([`crate::tls`]), never as a fact.
//!
//! Service type: `_clipmesh._tcp.local.`
//!
//! TXT record keys:
//!
//! | key | meaning |
//! | --- | --- |
//! | `id` | device id (UUIDv4) |
//! | `name` | display name |
//! | `platform` | `windows` / `linux` / `macos` / `android` |
//! | `version` | ClipMesh version |
//! | `fp` | certificate fingerprint, lowercase hex |

use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Duration;

use futures::stream::BoxStream;
use mdns_sd::{ResolvedService, ServiceDaemon, ServiceEvent, ServiceInfo};

use clipmesh_protocol::{DeviceId, DeviceInfo, Platform};

use crate::error::{NetworkError, Result};

/// The mDNS service type ClipMesh advertises.
pub const SERVICE_TYPE: &str = "_clipmesh._tcp.local.";

/// How long to wait for mDNS to settle before giving up on a lookup.
pub const RESOLVE_TIMEOUT: Duration = Duration::from_secs(5);

/// TXT record keys. Short on purpose: mDNS TXT records have a 1300 byte budget
/// for the whole record set and some stacks are stricter.
pub mod txt {
    /// Device id.
    pub const ID: &str = "id";
    /// Display name.
    pub const NAME: &str = "name";
    /// Platform token.
    pub const PLATFORM: &str = "platform";
    /// App version.
    pub const VERSION: &str = "version";
    /// Certificate fingerprint (hex).
    pub const FINGERPRINT: &str = "fp";
}

/// What we learned about a peer from its announcement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredService {
    /// Device id from the TXT record.
    pub device_id: DeviceId,
    /// Display name.
    pub name: String,
    /// Platform.
    pub platform: Platform,
    /// App version.
    pub version: String,
    /// Advertised fingerprint, lowercase hex. **Unverified.**
    pub fingerprint: String,
    /// Where to reach it.
    pub address: SocketAddr,
    /// The mDNS fullname, needed to remove the service later.
    pub fullname: String,
}

impl DiscoveredService {
    /// Wrap as the protocol level description.
    #[must_use]
    pub fn device_info(&self) -> DeviceInfo {
        DeviceInfo::new(
            self.device_id,
            self.name.clone(),
            self.platform,
            self.version.clone(),
        )
    }
}

/// Something mDNS told us.
#[derive(Debug, Clone)]
pub enum DiscoveryEvent {
    /// A peer was resolved to an address.
    Found(Box<DiscoveredService>),
    /// A peer announced it is going away.
    Lost {
        /// Full mDNS name of the departed service.
        fullname: String,
    },
    /// A non fatal discovery problem.
    Error(String),
}

/// Advertises this device and watches for others.
pub struct Discovery {
    daemon: ServiceDaemon,
    fullname: String,
}

impl Discovery {
    /// Start advertising `info` on `port` and browsing for peers.
    ///
    /// # Errors
    /// Returns [`NetworkError::Discovery`] when the mDNS daemon cannot start or
    /// the service cannot be registered.
    pub fn start(info: &DeviceInfo, fingerprint: &str, port: u16) -> Result<Self> {
        let daemon =
            ServiceDaemon::new().map_err(|error| NetworkError::Discovery(error.to_string()))?;

        let properties = HashMap::from([
            (txt::ID.to_owned(), info.device_id.to_string()),
            (txt::NAME.to_owned(), info.display_name().to_owned()),
            (txt::PLATFORM.to_owned(), info.platform.as_str().to_owned()),
            (txt::VERSION.to_owned(), info.app_version.clone()),
            (txt::FINGERPRINT.to_owned(), fingerprint.to_owned()),
        ]);

        // The instance name has to be unique on the network, and it is what
        // users see in other tools (`dns-sd -B`). Device id keeps it unique
        // even when two devices share a display name.
        let instance = format!("{}-{}", sanitize_instance(info.display_name()), short_id(info));

        let service = ServiceInfo::new(
            SERVICE_TYPE,
            &instance,
            &format!("{instance}.local."),
            "",
            port,
            properties,
        )
        .map_err(|error| NetworkError::Discovery(error.to_string()))?;

        // `enable_addr_auto` fills in our addresses from the interfaces, which
        // is what makes the record usable by other machines.
        let service = service.enable_addr_auto();

        let fullname = service.get_fullname().to_owned();

        daemon
            .register(service)
            .map_err(|error| NetworkError::Discovery(error.to_string()))?;

        tracing::info!(%fullname, port, "advertising over mDNS");

        Ok(Self { daemon, fullname })
    }

    /// Browse for other ClipMesh devices.
    ///
    /// The returned stream runs for the lifetime of the daemon. mDNS repeats
    /// announcements periodically, so a peer that was missed once will show up
    /// again without any retry logic on our side.
    ///
    /// # Errors
    /// Returns [`NetworkError::Discovery`] when browsing cannot start.
    pub fn browse(&self) -> Result<BoxStream<'static, DiscoveryEvent>> {
        let receiver = self
            .daemon
            .browse(SERVICE_TYPE)
            .map_err(|error| NetworkError::Discovery(error.to_string()))?;

        let stream = futures::stream::unfold(receiver, |receiver| async move {
            loop {
                match receiver.recv_async().await {
                    Ok(event) => match translate(event) {
                        Some(discovery_event) => return Some((discovery_event, receiver)),
                        // ServiceFound / ServiceRemoved mid-flight: keep polling.
                        None => continue,
                    },
                    Err(_) => return None,
                }
            }
        });

        Ok(Box::pin(stream))
    }

    /// The full name of our own registration.
    #[must_use]
    pub fn fullname(&self) -> &str {
        &self.fullname
    }

    /// Stop advertising and shut the daemon down.
    ///
    /// # Errors
    /// Returns [`NetworkError::Discovery`] when the daemon cannot be reached.
    pub fn shutdown(&self) -> Result<()> {
        self.daemon
            .unregister(&self.fullname)
            .map_err(|error| NetworkError::Discovery(error.to_string()))?;
        self.daemon
            .shutdown()
            .map_err(|error| NetworkError::Discovery(error.to_string()))?;
        Ok(())
    }
}

/// Turn an mdns-sd event into ours. Returns `None` for events we ignore.
///
/// `ServiceResolved` fires repeatedly for the same service (the address set
/// grows as more interfaces answer), so callers must treat it as a *liveness*
/// signal and deduplicate by device id rather than as "a new peer appeared".
/// `DeviceRegistry::observe` does exactly that.
fn translate(event: ServiceEvent) -> Option<DiscoveryEvent> {
    match event {
        ServiceEvent::ServiceResolved(resolved) => {
            let device_id = resolved
                .get_property_val_str(txt::ID)
                .and_then(|value| DeviceId::parse(value).ok())?;

            let address = pick_address(&resolved)?;

            Some(DiscoveryEvent::Found(Box::new(DiscoveredService {
                device_id,
                name: resolved
                    .get_property_val_str(txt::NAME)
                    .unwrap_or("ClipMesh device")
                    .to_owned(),
                platform: resolved
                    .get_property_val_str(txt::PLATFORM)
                    .map_or(Platform::Unspecified, Platform::parse),
                version: resolved
                    .get_property_val_str(txt::VERSION)
                    .unwrap_or_default()
                    .to_owned(),
                fingerprint: resolved
                    .get_property_val_str(txt::FINGERPRINT)
                    .unwrap_or_default()
                    .to_owned(),
                address,
                fullname: resolved.fullname.clone(),
            })))
        }
        ServiceEvent::ServiceRemoved(_service_type, fullname) => {
            Some(DiscoveryEvent::Lost { fullname })
        }
        // `ServiceFound`, `SearchStarted`, `SearchStopped` and anything a future
        // mdns-sd adds: nothing to do, the resolve event carries the payload.
        other => {
            tracing::trace!(?other, "ignoring an mDNS event");
            None
        }
    }
}

/// Prefer IPv4: every LAN that runs mDNS has it, and an IPv6 link-local address
/// carries a scope id that would have to be threaded through `connect`.
fn pick_address(resolved: &ResolvedService) -> Option<SocketAddr> {
    let ipv4 = resolved
        .addresses
        .iter()
        .map(|scoped| scoped.to_ip_addr())
        .find(|address| address.is_ipv4());

    let any = resolved
        .addresses
        .iter()
        .map(|scoped| scoped.to_ip_addr())
        .next();

    Some(SocketAddr::new(ipv4.or(any)?, resolved.port))
}

fn sanitize_instance(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .take(24)
        .collect();
    let trimmed = cleaned.trim_matches('-');
    if trimmed.is_empty() {
        "clipmesh".to_owned()
    } else {
        trimmed.to_owned()
    }
}

fn short_id(info: &DeviceInfo) -> String {
    info.device_id.to_string().chars().take(8).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instance_names_are_dns_safe() {
        assert_eq!(sanitize_instance("My Laptop!"), "My-Laptop");
        assert_eq!(sanitize_instance("  "), "clipmesh");
        assert_eq!(sanitize_instance(""), "clipmesh");
        assert!(sanitize_instance(&"x".repeat(100)).len() <= 24);
    }

    #[test]
    fn txt_keys_are_short_and_distinct() {
        let keys = [txt::ID, txt::NAME, txt::PLATFORM, txt::VERSION, txt::FINGERPRINT];
        for key in keys {
            assert!(key.len() <= 8, "{key} is too long for a TXT key");
        }
        let unique: std::collections::HashSet<&&str> = keys.iter().collect();
        assert_eq!(unique.len(), keys.len());
    }

    #[test]
    fn a_discovered_service_becomes_device_info() {
        let service = DiscoveredService {
            device_id: DeviceId::new(),
            name: "Pixel".into(),
            platform: Platform::Android,
            version: "0.1.0".into(),
            fingerprint: "abcd".into(),
            address: "192.168.0.9:47711".parse().unwrap(),
            fullname: "_clipmesh._tcp.local._x".into(),
        };
        let info = service.device_info();
        assert_eq!(info.device_id, service.device_id);
        assert_eq!(info.platform, Platform::Android);
        assert_eq!(info.name, "Pixel");
    }
}
