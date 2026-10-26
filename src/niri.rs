use std::collections::HashMap;
use std::fmt::{self, Display};
use std::path::PathBuf;
use std::time::Duration;

use async_io::Timer;
use iced::futures::channel::mpsc::Sender;
use iced::futures::sink::SinkExt;
use iced::futures::{Stream, StreamExt};
use iced::stream;
use niri_ipc::socket::SOCKET_PATH_ENV;
use niri_ipc::state::{EventStreamStatePart, KeyboardLayoutsState, WorkspacesState};
use niri_ipc::{Event, Reply, Request, Response};
use smol::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};
use smol::net::unix::UnixStream;
use smol::process::Command;

pub use niri_ipc::Workspace;

const RETRY_DELAY: Duration = Duration::from_secs(5);

pub type Update = Result<Snapshot, Error>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub workspaces: Vec<Workspace>,
    pub keyboard: Option<Keyboard>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keyboard {
    pub name: String,
    pub short_name: String,
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
        write!(f, "niri: {}", self.0)
    }
}

impl std::error::Error for Error {}

pub fn updates() -> impl Stream<Item = Update> {
    stream::channel(4, async |mut output| {
        // Layout descriptions rarely change, so resolve them once per subscription
        // rather than re-spawning xkbcli on every reconnect.
        let short_names = short_names().await;
        let mut delay = Duration::ZERO;

        loop {
            if !delay.is_zero() {
                Timer::after(delay).await;
            }

            match watch(&mut output, &short_names).await {
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

async fn watch(
    output: &mut Sender<Update>,
    short_names: &HashMap<String, String>,
) -> Result<(), Error> {
    let socket_path = std::env::var_os(SOCKET_PATH_ENV).ok_or_else(|| {
        Error::new(format!(
            "{SOCKET_PATH_ENV} is not set, are you running this within niri?"
        ))
    })?;

    let mut stream = UnixStream::connect(PathBuf::from(socket_path))
        .await
        .map_err(|error| Error::new(format!("failed to connect to niri: {error}")))?;

    let mut request = serde_json::to_vec(&Request::EventStream)
        .map_err(|error| Error::new(format!("failed to encode niri request: {error}")))?;
    request.push(b'\n');
    stream
        .write_all(&request)
        .await
        .map_err(|error| Error::new(format!("failed to request niri event stream: {error}")))?;
    stream
        .flush()
        .await
        .map_err(|error| Error::new(format!("failed to flush niri request: {error}")))?;

    let mut lines = BufReader::new(stream).lines();
    let reply = lines
        .next()
        .await
        .ok_or_else(|| Error::new("niri closed the socket before replying"))?
        .map_err(|error| Error::new(format!("failed to read niri reply: {error}")))?;
    let reply: Reply = serde_json::from_str(&reply)
        .map_err(|error| Error::new(format!("failed to decode niri reply: {error}")))?;

    match reply {
        Ok(Response::Handled) => {}
        Ok(response) => {
            return Err(Error::new(format!(
                "unexpected niri reply to event stream request: {response:?}"
            )));
        }
        Err(message) => {
            return Err(Error::new(format!(
                "niri rejected event stream request: {message}"
            )));
        }
    }

    let mut workspaces = WorkspacesState::default();
    let mut keyboard = KeyboardLayoutsState::default();

    while let Some(line) = lines.next().await {
        let line =
            line.map_err(|error| Error::new(format!("failed to read niri event: {error}")))?;
        let event: Event = serde_json::from_str(&line)
            .map_err(|error| Error::new(format!("failed to decode niri event: {error}")))?;

        if !is_relevant(&event) {
            continue;
        }

        apply_event(&mut workspaces, &mut keyboard, event);
        forward(output, snapshot(&workspaces, &keyboard, short_names)).await?;
    }

    Err(Error::new("niri stopped sending events"))
}

fn is_relevant(event: &Event) -> bool {
    matches!(
        event,
        Event::WorkspacesChanged { .. }
            | Event::WorkspaceActivated { .. }
            | Event::WorkspaceUrgencyChanged { .. }
            | Event::KeyboardLayoutsChanged { .. }
            | Event::KeyboardLayoutSwitched { .. }
    )
}

fn apply_event(
    workspaces: &mut WorkspacesState,
    keyboard: &mut KeyboardLayoutsState,
    event: Event,
) {
    let event = match workspaces.apply(event) {
        Some(event) => event,
        None => return,
    };
    let _ = keyboard.apply(event);
}

fn snapshot(
    workspaces: &WorkspacesState,
    keyboard: &KeyboardLayoutsState,
    short_names: &HashMap<String, String>,
) -> Snapshot {
    let mut workspaces: Vec<_> = workspaces.workspaces.values().cloned().collect();
    workspaces.sort_by(|a, b| a.output.cmp(&b.output).then_with(|| a.idx.cmp(&b.idx)));

    let keyboard = keyboard.keyboard_layouts.as_ref().and_then(|layouts| {
        let name = layouts.names.get(usize::from(layouts.current_idx))?.clone();
        let short_name = short_names
            .get(&name)
            .cloned()
            .unwrap_or_else(|| name.clone());

        Some(Keyboard { name, short_name })
    });

    Snapshot {
        workspaces,
        keyboard,
    }
}

async fn forward(output: &mut Sender<Update>, snapshot: Snapshot) -> Result<(), Error> {
    output
        .send(Ok(snapshot))
        .await
        .map_err(|_| Error::new("update channel closed"))
}

async fn short_names() -> HashMap<String, String> {
    let output = Command::new("xkbcli").arg("list").output().await;
    let Ok(output) = output else {
        return HashMap::new();
    };
    if !output.status.success() {
        return HashMap::new();
    }

    parse_short_names(&String::from_utf8_lossy(&output.stdout))
}

/// Maps xkb layout descriptions, e.g. "English (US)", to their short names, e.g. "us".
fn parse_short_names(output: &str) -> HashMap<String, String> {
    let Some((_, layouts)) = output.split_once("layouts:\n") else {
        return HashMap::new();
    };
    let layouts = layouts
        .split_once("\noption_groups:")
        .map_or(layouts, |(layouts, _)| layouts);

    layouts
        .split("\n- layout: ")
        .filter_map(|block| {
            let block = block.strip_prefix("- layout: ").unwrap_or(block);
            let short_name = block.lines().next()?.trim().trim_matches('\'');
            let name = block
                .lines()
                .find_map(|line| line.trim().strip_prefix("description: "))?;

            Some((name.to_owned(), short_name.to_owned()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use niri_ipc::KeyboardLayouts;

    fn workspace(id: u64, idx: u8, output: &str) -> Workspace {
        Workspace {
            id,
            idx,
            name: None,
            output: Some(output.to_owned()),
            is_urgent: false,
            is_active: false,
            is_focused: false,
            active_window_id: None,
        }
    }

    #[test]
    fn parses_short_names_from_xkbcli_list() {
        let output = "\
models:
- name: pc105
  vendor: Generic
  description: Generic 105-key PC
layouts:
- layout: 'us'
  variant: ''
  brief: 'en'
  description: English (US)
  iso639: ['eng']
  iso3166: ['US']
- layout: 'ru'
  variant: ''
  brief: 'ru'
  description: Russian
  iso639: ['rus']
  iso3166: ['RU']
option_groups:
- name: 'grp'
";

        let names = parse_short_names(output);

        assert_eq!(names.get("English (US)"), Some(&"us".to_owned()));
        assert_eq!(names.get("Russian"), Some(&"ru".to_owned()));
    }

    #[test]
    fn snapshot_sorts_workspaces_and_maps_keyboard_layout() {
        let mut workspaces = WorkspacesState::default();
        for workspace in [
            workspace(1, 2, "B"),
            workspace(2, 1, "B"),
            workspace(3, 1, "A"),
        ] {
            workspaces.workspaces.insert(workspace.id, workspace);
        }

        let keyboard = KeyboardLayoutsState {
            keyboard_layouts: Some(KeyboardLayouts {
                names: vec!["English (US)".to_owned(), "Russian".to_owned()],
                current_idx: 1,
            }),
        };
        let short_names = HashMap::from([("Russian".to_owned(), "ru".to_owned())]);

        let snapshot = snapshot(&workspaces, &keyboard, &short_names);

        assert_eq!(
            snapshot
                .workspaces
                .iter()
                .map(|workspace| workspace.id)
                .collect::<Vec<_>>(),
            vec![3, 2, 1]
        );
        assert_eq!(snapshot.keyboard.unwrap().short_name, "ru");
    }
}
