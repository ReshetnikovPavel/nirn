use std::fmt::{self, Display};
use std::time::Duration;

use async_io::Timer;
use iced::futures::channel::mpsc::Sender;
use iced::futures::sink::SinkExt;
use iced::futures::{Stream, StreamExt};
use iced::stream;
use zbus::fdo::PropertiesProxy;
use zbus::proxy;
use zbus::proxy::CacheProperties;
use zbus::Connection;

const SERVICE: &str = "org.freedesktop.UPower";
const DEVICE_PATH: &str = "/org/freedesktop/UPower/devices/DisplayDevice";
const RELEVANT_PROPERTIES: [&str; 3] = ["Percentage", "State", "IsPresent"];

const RETRY_DELAY: Duration = Duration::from_secs(5);

pub type Update = Result<Option<Snapshot>, Error>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Charging,
    NotCharging,
    Discharging,
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Snapshot {
    pub status: Status,
    pub percentage: i64,
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
        write!(f, "battery: {}", self.0)
    }
}

impl std::error::Error for Error {}

impl From<zbus::Error> for Error {
    fn from(error: zbus::Error) -> Self {
        Self(error.to_string())
    }
}

#[proxy(
    interface = "org.freedesktop.UPower.Device",
    default_service = "org.freedesktop.UPower",
    default_path = "/org/freedesktop/UPower/devices/DisplayDevice",
    gen_blocking = false
)]
trait UpowerDevice {
    #[zbus(property)]
    fn percentage(&self) -> zbus::Result<f64>;

    #[zbus(property)]
    fn state(&self) -> zbus::Result<u32>;

    #[zbus(property)]
    fn is_present(&self) -> zbus::Result<bool>;
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
    let device = UpowerDeviceProxy::builder(&connection)
        .cache_properties(CacheProperties::No)
        .build()
        .await?;
    let properties = PropertiesProxy::builder(&connection)
        .destination(SERVICE)?
        .path(DEVICE_PATH)?
        .cache_properties(CacheProperties::No)
        .build()
        .await?;
    let mut changed = properties.receive_properties_changed().await?;

    forward(output, &device).await?;

    while let Some(message) = changed.next().await {
        let Ok(args) = message.args() else {
            continue;
        };
        if !args
            .changed_properties()
            .keys()
            .any(|name| RELEVANT_PROPERTIES.contains(name))
        {
            continue;
        }

        forward(output, &device).await?;
    }

    Err(Error::new("UPower stopped sending property updates"))
}

async fn forward(
    output: &mut Sender<Update>,
    device: &UpowerDeviceProxy<'_>,
) -> Result<(), Error> {
    match read(device).await {
        Ok(update) => output
            .send(Ok(update))
            .await
            .map_err(|_| Error::new("update channel closed")),
        Err(error) => Err(error),
    }
}

async fn read(device: &UpowerDeviceProxy<'_>) -> Result<Option<Snapshot>, Error> {
    if !device.is_present().await? {
        return Ok(None);
    }

    let percentage = device.percentage().await?;
    let state = device.state().await?;

    Ok(Some(Snapshot {
        status: status(state),
        percentage: percentage.round().clamp(0.0, 100.0) as i64,
    }))
}

fn status(state: u32) -> Status {
    match state {
        1 => Status::Charging,
        2 => Status::Discharging,
        4 => Status::Full,
        _ => Status::NotCharging,
    }
}
