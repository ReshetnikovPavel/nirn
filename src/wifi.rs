use std::fmt::{self, Display};
use std::time::Duration;

use async_io::Timer;
use iced::futures::channel::mpsc::Sender;
use iced::futures::select;
use iced::futures::sink::SinkExt;
use iced::futures::{Stream, StreamExt};
use iced::stream;
use zbus::Connection;
use zbus::fdo::{PropertiesChanged, PropertiesChangedStream, PropertiesProxy};
use zbus::proxy;
use zbus::proxy::CacheProperties;
use zbus::zvariant::OwnedObjectPath;

const SERVICE: &str = "org.freedesktop.NetworkManager";
const MAIN_PATH: &str = "/org/freedesktop/NetworkManager";
const WIFI_DEVICE_TYPE: u32 = 2;
const RETRY_DELAY: Duration = Duration::from_secs(5);

const MANAGER_INTERFACE: &str = "org.freedesktop.NetworkManager";
const WIRELESS_INTERFACE: &str = "org.freedesktop.NetworkManager.Device.Wireless";
const ACCESS_POINT_INTERFACE: &str = "org.freedesktop.NetworkManager.AccessPoint";

pub type Update = Result<Option<Snapshot>, Error>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub ssid: String,
    pub strength: u8,
}

#[derive(Debug, Clone)]
pub struct Error(String);

impl Error {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "wifi: {}", self.0)
    }
}

impl std::error::Error for Error {}

impl From<zbus::Error> for Error {
    fn from(error: zbus::Error) -> Self {
        Self(error.to_string())
    }
}

#[proxy(
    interface = "org.freedesktop.NetworkManager",
    default_service = "org.freedesktop.NetworkManager",
    default_path = "/org/freedesktop/NetworkManager",
    gen_blocking = false
)]
trait NetworkManager {
    #[zbus(property)]
    fn devices(&self) -> zbus::Result<Vec<OwnedObjectPath>>;
}

#[proxy(
    interface = "org.freedesktop.NetworkManager.Device",
    default_service = "org.freedesktop.NetworkManager",
    default_path = "/org/freedesktop/NetworkManager/Devices/0",
    gen_blocking = false
)]
trait Device {
    #[zbus(property)]
    fn device_type(&self) -> zbus::Result<u32>;
}

#[proxy(
    interface = "org.freedesktop.NetworkManager.Device.Wireless",
    default_service = "org.freedesktop.NetworkManager",
    default_path = "/org/freedesktop/NetworkManager/Devices/0",
    gen_blocking = false
)]
trait Wireless {
    #[zbus(property)]
    fn active_access_point(&self) -> zbus::Result<OwnedObjectPath>;
}

#[proxy(
    interface = "org.freedesktop.NetworkManager.AccessPoint",
    default_service = "org.freedesktop.NetworkManager",
    default_path = "/org/freedesktop/NetworkManager/AccessPoint/0",
    gen_blocking = false
)]
trait AccessPoint {
    #[zbus(property)]
    fn ssid(&self) -> zbus::Result<Vec<u8>>;

    #[zbus(property)]
    fn strength(&self) -> zbus::Result<u8>;
}

pub fn updates() -> impl Stream<Item = Update> {
    stream::channel(4, async |mut output| {
        let mut delay = Duration::ZERO;

        loop {
            if !delay.is_zero() {
                Timer::after(delay).await;
            }

            match watch(&mut output).await {
                Ok(()) => break,
                Err(error) => {
                    if output.send(Err(error)).await.is_err() {
                        break;
                    }
                    delay = RETRY_DELAY;
                }
            }
        }
    })
}

async fn watch(output: &mut Sender<Update>) -> Result<(), Error> {
    let connection = Connection::system().await?;
    let manager = NetworkManagerProxy::builder(&connection)
        .cache_properties(CacheProperties::No)
        .build()
        .await?;
    let manager_properties = PropertiesProxy::builder(&connection)
        .destination(SERVICE)?
        .path(MAIN_PATH)?
        .cache_properties(CacheProperties::No)
        .build()
        .await?;
    let mut manager_changed = manager_properties.receive_properties_changed().await?;

    'device: loop {
        let wifi_path = find_wifi_device(&connection, manager.devices().await?).await?;

        let Some(wifi_path) = wifi_path else {
            forward(output, None).await?;
            wait_for_property(&mut manager_changed, MANAGER_INTERFACE, "Devices").await?;
            continue;
        };

        let wireless = WirelessProxy::builder(&connection)
            .path(wifi_path.clone())?
            .cache_properties(CacheProperties::No)
            .build()
            .await?;
        let wireless_properties = PropertiesProxy::builder(&connection)
            .destination(SERVICE)?
            .path(wifi_path)?
            .cache_properties(CacheProperties::No)
            .build()
            .await?;
        let mut wireless_changed = wireless_properties.receive_properties_changed().await?;

        loop {
            let active_access_point = wireless.active_access_point().await?;

            if active_access_point.as_str() == "/" {
                forward(output, None).await?;

                loop {
                    select! {
                        manager_change = manager_changed.next() => {
                            let Some(message) = manager_change else {
                                return Err(Error::new(
                                    "NetworkManager stopped sending device updates",
                                ));
                            };

                            if changes_property(&message, MANAGER_INTERFACE, "Devices") {
                                continue 'device;
                            }
                        }
                        wireless_change = wireless_changed.next() => {
                            let Some(message) = wireless_change else {
                                continue 'device;
                            };

                            if changes_property(&message, WIRELESS_INTERFACE, "ActiveAccessPoint") {
                                break;
                            }
                        }
                    }
                }

                continue;
            }

            let access_point = AccessPointProxy::builder(&connection)
                .path(active_access_point.clone())?
                .cache_properties(CacheProperties::No)
                .build()
                .await?;
            let access_point_properties = PropertiesProxy::builder(&connection)
                .destination(SERVICE)?
                .path(active_access_point)?
                .cache_properties(CacheProperties::No)
                .build()
                .await?;
            let mut access_point_changed =
                access_point_properties.receive_properties_changed().await?;

            forward_access_point(output, &access_point).await?;

            loop {
                select! {
                    manager_change = manager_changed.next() => {
                        let Some(message) = manager_change else {
                            return Err(Error::new(
                                "NetworkManager stopped sending device updates",
                            ));
                        };

                        if changes_property(&message, MANAGER_INTERFACE, "Devices") {
                            continue 'device;
                        }
                    }
                    wireless_change = wireless_changed.next() => {
                        let Some(message) = wireless_change else {
                            continue 'device;
                        };

                        if changes_property(&message, WIRELESS_INTERFACE, "ActiveAccessPoint") {
                            break;
                        }
                    }
                    access_point_change = access_point_changed.next() => {
                        let Some(message) = access_point_change else {
                            break;
                        };

                        if changes_property(&message, ACCESS_POINT_INTERFACE, "Strength") {
                            forward_access_point(output, &access_point).await?;
                        }
                    }
                }
            }
        }
    }
}

async fn find_wifi_device(
    connection: &Connection,
    devices: Vec<OwnedObjectPath>,
) -> Result<Option<OwnedObjectPath>, Error> {
    for path in devices {
        let device = DeviceProxy::builder(connection)
            .path(path.clone())?
            .cache_properties(CacheProperties::No)
            .build()
            .await?;

        let Ok(device_type) = device.device_type().await else {
            continue;
        };

        if device_type == WIFI_DEVICE_TYPE {
            return Ok(Some(path));
        }
    }

    Ok(None)
}

async fn forward_access_point(
    output: &mut Sender<Update>,
    access_point: &AccessPointProxy<'_>,
) -> Result<(), Error> {
    let ssid = ssid(&access_point.ssid().await?);
    let strength = access_point.strength().await?;

    forward(output, ssid.map(|ssid| Snapshot { ssid, strength })).await
}

async fn forward(output: &mut Sender<Update>, snapshot: Option<Snapshot>) -> Result<(), Error> {
    output
        .send(Ok(snapshot))
        .await
        .map_err(|_| Error::new("update channel closed"))
}

async fn wait_for_property(
    changes: &mut PropertiesChangedStream,
    interface: &str,
    property: &str,
) -> Result<(), Error> {
    while let Some(message) = changes.next().await {
        if changes_property(&message, interface, property) {
            return Ok(());
        }
    }

    Err(Error::new("NetworkManager stopped sending updates"))
}

fn changes_property(message: &PropertiesChanged, interface: &str, property: &str) -> bool {
    let Ok(args) = message.args() else {
        return false;
    };

    args.interface_name().as_str() == interface
        && (args.changed_properties().contains_key(property)
            || args.invalidated_properties().contains(&property))
}

fn ssid(bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() {
        None
    } else {
        Some(String::from_utf8_lossy(bytes).into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_ssid_bytes() {
        assert_eq!(ssid(b"pavelresh"), Some("pavelresh".to_owned()));
    }

    #[test]
    fn ignores_empty_ssid() {
        assert_eq!(ssid(b""), None);
    }
}
