use std::fmt::{self, Display};
use std::path::{Path, PathBuf};
use std::time::Duration;

use async_io::{Async, Timer};
use iced::futures::Stream;
use iced::futures::channel::mpsc::Sender;
use iced::futures::sink::SinkExt;
use iced::stream;
use inotify::{EventMask, Inotify, WatchMask};

const BACKLIGHT_DIR: &str = "/sys/class/backlight";
const RETRY_DELAY: Duration = Duration::from_secs(5);

pub type Update = Result<Option<Snapshot>, Error>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Snapshot {
    pub percentage: u8,
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
        write!(f, "brightness: {}", self.0)
    }
}

impl std::error::Error for Error {}

struct Backlight {
    brightness_path: PathBuf,
    max_brightness: u64,
}

impl Backlight {
    fn open(path: PathBuf) -> Option<Self> {
        let max_brightness = std::fs::read_to_string(path.join("max_brightness"))
            .ok()?
            .trim()
            .parse()
            .ok()?;
        if max_brightness == 0 {
            return None;
        }

        let brightness_path = path.join("brightness");
        brightness_path.metadata().ok()?;

        Some(Self {
            brightness_path,
            max_brightness,
        })
    }

    async fn read(&self) -> Result<Snapshot, Error> {
        let brightness = read_number(&self.brightness_path).await?;

        Ok(Snapshot {
            percentage: percentage(brightness, self.max_brightness),
        })
    }
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
    let Some(backlight) = find_backlight() else {
        output
            .send(Ok(None))
            .await
            .map_err(|_| Error::new("update channel closed"))?;
        return Ok(());
    };

    let inotify = Inotify::init()
        .map_err(|error| Error::new(format!("failed to initialize inotify: {error}")))?;
    let watch_mask = WatchMask::MODIFY | WatchMask::ATTRIB | WatchMask::CLOSE_WRITE;
    inotify
        .watches()
        .add(&backlight.brightness_path, watch_mask)
        .map_err(|error| {
            Error::new(format!(
                "failed to watch {}: {error}",
                backlight.brightness_path.display()
            ))
        })?;
    let mut inotify = Async::new(inotify)
        .map_err(|error| Error::new(format!("failed to register inotify: {error}")))?;

    let mut last = None;
    forward_if_changed(output, &mut last, Some(backlight.read().await?)).await?;

    let mut buffer = [0; 4096];
    loop {
        inotify
            .readable()
            .await
            .map_err(|error| Error::new(format!("inotify wait failed: {error}")))?;

        let changed = {
            // SAFETY: `inotify` is only used for reading here; its file descriptor
            // is not replaced or closed while the registration is active.
            let inotify = unsafe { inotify.get_mut() };
            let mut events = match inotify.read_events(&mut buffer) {
                Ok(events) => events,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => continue,
                Err(error) => {
                    return Err(Error::new(format!(
                        "failed to read inotify events: {error}"
                    )));
                }
            };

            events.any(|event| {
                event
                    .mask
                    .intersects(EventMask::MODIFY | EventMask::ATTRIB | EventMask::CLOSE_WRITE)
            })
        };

        if changed {
            forward_if_changed(output, &mut last, Some(backlight.read().await?)).await?;
        }
    }
}

async fn forward_if_changed(
    output: &mut Sender<Update>,
    last: &mut Option<Snapshot>,
    snapshot: Option<Snapshot>,
) -> Result<(), Error> {
    if *last != snapshot {
        *last = snapshot;
        output
            .send(Ok(snapshot))
            .await
            .map_err(|_| Error::new("update channel closed"))?;
    }

    Ok(())
}

fn find_backlight() -> Option<Backlight> {
    let mut paths: Vec<_> = std::fs::read_dir(BACKLIGHT_DIR)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();

    paths.sort_by(|a, b| {
        backlight_priority(a)
            .cmp(&backlight_priority(b))
            .then_with(|| a.cmp(b))
    });

    paths.into_iter().find_map(Backlight::open)
}

fn backlight_priority(path: &Path) -> u8 {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_default();

    if name == "intel_backlight" {
        0
    } else if name.starts_with("amdgpu_bl") {
        1
    } else {
        2
    }
}

async fn read_number(path: &Path) -> Result<u64, Error> {
    let value = smol::fs::read_to_string(path)
        .await
        .map_err(|error| Error::new(format!("failed to read {}: {error}", path.display())))?;

    value.trim().parse().map_err(|error| {
        Error::new(format!(
            "invalid brightness value in {}: {error}",
            path.display()
        ))
    })
}

fn percentage(brightness: u64, max_brightness: u64) -> u8 {
    if max_brightness == 0 {
        return 0;
    }

    let rounded = (brightness.saturating_mul(100) + max_brightness / 2) / max_brightness;

    rounded.min(100) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_percentage() {
        assert_eq!(percentage(0, 96_000), 0);
        assert_eq!(percentage(57_600, 96_000), 60);
        assert_eq!(percentage(96_000, 96_000), 100);
        assert_eq!(percentage(100_000, 96_000), 100);
    }
}
