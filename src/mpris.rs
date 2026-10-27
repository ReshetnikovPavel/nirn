use std::collections::HashMap;
use std::fmt::{self, Display};
use std::time::Duration;

use async_io::Timer;
use iced::futures::channel::mpsc::Sender;
use iced::futures::select;
use iced::futures::sink::SinkExt;
use iced::futures::{Stream, StreamExt};
use iced::stream;
use zbus::fdo::{DBusProxy, NameOwnerChangedStream, PropertiesChanged};
use zbus::message::Type;
use zbus::proxy;
use zbus::proxy::CacheProperties;
use zbus::zvariant::{OwnedValue, Value};
use zbus::{Connection, MatchRule, MessageStream};

const MPRIS_PREFIX: &str = "org.mpris.MediaPlayer2.";
const PLAYER_INTERFACE: &str = "org.mpris.MediaPlayer2.Player";
const RETRY_DELAY: Duration = Duration::from_secs(5);

pub type Update = Result<Option<Snapshot>, Error>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub track: String,
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
        write!(f, "mpris: {}", self.0)
    }
}

impl std::error::Error for Error {}

impl From<zbus::Error> for Error {
    fn from(error: zbus::Error) -> Self {
        Self(error.to_string())
    }
}

impl From<zbus::fdo::Error> for Error {
    fn from(error: zbus::fdo::Error) -> Self {
        Self(error.to_string())
    }
}

#[proxy(
    interface = "org.mpris.MediaPlayer2.Player",
    default_service = "org.mpris.MediaPlayer2.player",
    default_path = "/org/mpris/MediaPlayer2",
    gen_blocking = false
)]
trait Player {
    #[zbus(property)]
    fn metadata(&self) -> zbus::Result<HashMap<String, OwnedValue>>;

    #[zbus(property)]
    fn playback_status(&self) -> zbus::Result<String>;
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
    let connection = Connection::session().await?;
    let dbus = DBusProxy::new(&connection).await?;
    let mut name_changed = dbus.receive_name_owner_changed().await?;

    let rule = MatchRule::builder()
        .msg_type(Type::Signal)
        .interface("org.freedesktop.DBus.Properties")?
        .member("PropertiesChanged")?
        .arg(0, PLAYER_INTERFACE)?
        .build();
    let mut property_changes = MessageStream::for_match_rule(rule, &connection, Some(64)).await?;

    loop {
        let names = mpris_names(&dbus).await?;
        forward(output, best_snapshot(&connection, &names).await).await?;
        wait_for_change(&mut name_changed, &mut property_changes).await?;
    }
}

async fn mpris_names(dbus: &DBusProxy<'_>) -> Result<Vec<String>, Error> {
    let mut names: Vec<String> = dbus
        .list_names()
        .await?
        .into_iter()
        .map(|name| name.to_string())
        .filter(|name| name.starts_with(MPRIS_PREFIX))
        .collect();

    names.sort();
    Ok(names)
}

async fn best_snapshot(connection: &Connection, names: &[String]) -> Option<Snapshot> {
    let mut fallback = None;
    let mut paused = None;

    for name in names {
        let Ok(builder) = PlayerProxy::builder(connection).destination(name.as_str()) else {
            continue;
        };
        let Ok(player) = builder.cache_properties(CacheProperties::No).build().await else {
            continue;
        };

        let status = player.playback_status().await.unwrap_or_default();
        let snapshot = read(&player).await.ok().flatten();

        if status == "Playing" && snapshot.is_some() {
            return snapshot;
        }

        if status == "Paused" && paused.is_none() {
            paused = snapshot.clone();
        }

        if fallback.is_none() {
            fallback = snapshot;
        }
    }

    paused.or(fallback)
}

async fn forward(output: &mut Sender<Update>, snapshot: Option<Snapshot>) -> Result<(), Error> {
    output
        .send(Ok(snapshot))
        .await
        .map_err(|_| Error::new("update channel closed"))
}

async fn read(player: &PlayerProxy<'_>) -> Result<Option<Snapshot>, Error> {
    let metadata = player.metadata().await?;
    Ok(track(&metadata))
}

async fn wait_for_change(
    name_changed: &mut NameOwnerChangedStream,
    property_changes: &mut MessageStream,
) -> Result<(), Error> {
    loop {
        select! {
            name_change = name_changed.next() => {
                let Some(message) = name_change else {
                    return Err(Error::new("D-Bus stopped sending name updates"));
                };

                let Ok(args) = message.args() else {
                    continue;
                };

                if args.name().as_str().starts_with(MPRIS_PREFIX) {
                    return Ok(());
                }
            }
            property_change = property_changes.next() => {
                let Some(message) = property_change else {
                    return Err(Error::new("D-Bus stopped sending property updates"));
                };

                let Ok(message) = message else {
                    continue;
                };

                let Some(signal) = PropertiesChanged::from_message(message) else {
                    continue;
                };

                let Ok(args) = signal.args() else {
                    continue;
                };

                if args.interface_name().as_str() != PLAYER_INTERFACE {
                    continue;
                }

                let changed = args.changed_properties();
                let invalidated = args.invalidated_properties();
                let relevant = changed.contains_key("Metadata")
                    || invalidated.contains(&"Metadata")
                    || changed.contains_key("PlaybackStatus")
                    || invalidated.contains(&"PlaybackStatus");

                if relevant {
                    return Ok(());
                }
            }
        }
    }
}

fn track(metadata: &HashMap<String, OwnedValue>) -> Option<Snapshot> {
    let title = metadata
        .get("xesam:title")
        .and_then(string_value)
        .and_then(clean)?;

    let artist = metadata.get("xesam:artist").and_then(artist_value);
    let track = match artist {
        Some(artist) => format!("{artist} – {title}"),
        None => title,
    };

    Some(Snapshot { track })
}

fn string_value(value: &OwnedValue) -> Option<String> {
    value.downcast_ref::<String>().ok()
}

fn artist_value(value: &OwnedValue) -> Option<String> {
    match &**value {
        Value::Str(artist) => clean(artist.to_string()),
        Value::Array(array) => {
            let artists: Vec<String> = array
                .iter()
                .filter_map(|value| match value {
                    Value::Str(artist) => clean(artist.to_string()),
                    _ => None,
                })
                .collect();

            if artists.is_empty() {
                None
            } else {
                Some(artists.join(", "))
            }
        }
        _ => None,
    }
}

fn clean(value: String) -> Option<String> {
    let value = value
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    (!value.is_empty()).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::{Array, Str};

    fn metadata(title: &str, artist: Option<&[&str]>) -> HashMap<String, OwnedValue> {
        let mut metadata = HashMap::new();
        metadata.insert("xesam:title".to_owned(), OwnedValue::from(Str::from(title)));

        if let Some(artist) = artist {
            let artist = Array::from(
                artist
                    .iter()
                    .map(|artist| Str::from(*artist))
                    .collect::<Vec<_>>(),
            );
            metadata.insert(
                "xesam:artist".to_owned(),
                OwnedValue::try_from(artist).unwrap(),
            );
        }

        metadata
    }

    #[test]
    fn formats_track_with_artist() {
        let metadata = metadata("Track", Some(&["Artist"]));

        assert_eq!(
            track(&metadata),
            Some(Snapshot {
                track: "Artist – Track".to_owned()
            })
        );
    }

    #[test]
    fn formats_track_without_artist() {
        let metadata = metadata("Track", None);

        assert_eq!(
            track(&metadata),
            Some(Snapshot {
                track: "Track".to_owned()
            })
        );
    }

    #[test]
    fn joins_multiple_artists() {
        let metadata = metadata("Track", Some(&["Artist", "Other"]));

        assert_eq!(
            track(&metadata),
            Some(Snapshot {
                track: "Artist, Other – Track".to_owned()
            })
        );
    }

    #[test]
    fn collapses_line_breaks() {
        let metadata = metadata("Line one\nLine two", Some(&["Artist\nOther"]));

        assert_eq!(
            track(&metadata),
            Some(Snapshot {
                track: "Artist Other – Line one Line two".to_owned()
            })
        );
    }

    #[test]
    fn ignores_empty_title() {
        let metadata = metadata("  ", Some(&["Artist"]));

        assert_eq!(track(&metadata), None);
    }
}
