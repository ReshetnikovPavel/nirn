use std::fmt::{self, Display};
use std::time::Duration;

use async_io::Timer;
use iced::futures::channel::mpsc::Sender;
use iced::futures::sink::SinkExt;
use iced::futures::{Stream, StreamExt};
use iced::stream;
use serde::Deserialize;
use smol::io::{AsyncBufReadExt as _, BufReader};
use smol::process::{Command, Stdio};

const RETRY_DELAY: Duration = Duration::from_secs(5);

pub type Update = Result<Snapshot, Error>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub workspaces: Vec<Workspace>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Workspace {
    pub id: u64,
    pub idx: u8,
    pub name: Option<String>,
    pub output: Option<String>,
    pub is_urgent: bool,
    pub is_active: bool,
    pub is_focused: bool,
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
        write!(f, "workspaces: {}", self.0)
    }
}

impl std::error::Error for Error {}

#[derive(Debug, Deserialize)]
struct Event {
    #[serde(rename = "WorkspacesChanged")]
    workspaces_changed: Option<WorkspacesChanged>,
    #[serde(rename = "WorkspaceActivated")]
    workspace_activated: Option<WorkspaceActivated>,
    #[serde(rename = "WorkspaceUrgencyChanged")]
    workspace_urgency_changed: Option<WorkspaceUrgencyChanged>,
}

#[derive(Debug, Deserialize)]
struct WorkspacesChanged {
    workspaces: Vec<Workspace>,
}

#[derive(Debug, Deserialize)]
struct WorkspaceActivated {
    id: u64,
    focused: bool,
}

#[derive(Debug, Deserialize)]
struct WorkspaceUrgencyChanged {
    id: u64,
    urgent: bool,
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
    let mut child = Command::new("niri")
        .args(["msg", "--json", "event-stream"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| Error::new(format!("failed to start niri: {error}")))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| Error::new("niri did not provide stdout"))?;
    let mut lines = BufReader::new(stdout).lines();

    let mut workspaces = Vec::new();
    while let Some(line) = lines.next().await {
        let line = line.map_err(|error| Error::new(format!("failed to read event: {error}")))?;
        let Ok(event) = serde_json::from_str::<Event>(&line) else {
            continue;
        };

        let changed = if let Some(changed) = event.workspaces_changed {
            workspaces = changed.workspaces;
            workspaces.sort_by(|a, b| a.output.cmp(&b.output).then_with(|| a.idx.cmp(&b.idx)));
            true
        } else if let Some(activated) = event.workspace_activated {
            activate(&mut workspaces, activated);
            true
        } else if let Some(urgency) = event.workspace_urgency_changed {
            if let Some(workspace) = workspaces.iter_mut().find(|w| w.id == urgency.id) {
                workspace.is_urgent = urgency.urgent;
            }
            true
        } else {
            false
        };

        if changed {
            forward(output, &workspaces).await?;
        }
    }

    Err(Error::new("niri stopped sending events"))
}

fn activate(workspaces: &mut [Workspace], activated: WorkspaceActivated) {
    let Some(target) = workspaces.iter().find(|w| w.id == activated.id) else {
        return;
    };
    let output = target.output.clone();

    for workspace in workspaces.iter_mut() {
        if workspace.output == output {
            workspace.is_active = false;
        }
        if activated.focused {
            workspace.is_focused = false;
        }
    }

    if let Some(workspace) = workspaces.iter_mut().find(|w| w.id == activated.id) {
        workspace.is_active = true;
        if activated.focused {
            workspace.is_focused = true;
        }
    }
}

async fn forward(output: &mut Sender<Update>, workspaces: &[Workspace]) -> Result<(), Error> {
    output
        .send(Ok(Snapshot {
            workspaces: workspaces.to_vec(),
        }))
        .await
        .map_err(|_| Error::new("update channel closed"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace(id: u64, idx: u8, is_active: bool, is_focused: bool) -> Workspace {
        Workspace {
            id,
            idx,
            name: None,
            output: Some("eDP-1".to_owned()),
            is_urgent: false,
            is_active,
            is_focused,
        }
    }

    #[test]
    fn activating_workspace_updates_active_and_focused_flags() {
        let mut workspaces = vec![
            workspace(1, 1, false, false),
            workspace(2, 2, true, true),
            workspace(3, 3, false, false),
        ];

        activate(
            &mut workspaces,
            WorkspaceActivated {
                id: 1,
                focused: true,
            },
        );

        assert!(workspaces[0].is_active);
        assert!(workspaces[0].is_focused);
        assert!(!workspaces[1].is_active);
        assert!(!workspaces[1].is_focused);
        assert!(!workspaces[2].is_active);
        assert!(!workspaces[2].is_focused);
    }
}
