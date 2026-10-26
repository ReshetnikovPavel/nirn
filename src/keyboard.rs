use std::collections::HashMap;
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
        write!(f, "keyboard: {}", self.0)
    }
}

impl std::error::Error for Error {}

#[derive(Debug, Deserialize)]
struct Event {
    #[serde(rename = "KeyboardLayoutsChanged")]
    layouts_changed: Option<LayoutsChanged>,
    #[serde(rename = "KeyboardLayoutSwitched")]
    layout_switched: Option<LayoutSwitched>,
}

#[derive(Debug, Deserialize)]
struct LayoutsChanged {
    keyboard_layouts: Layouts,
}

#[derive(Debug, Clone, Deserialize)]
struct Layouts {
    names: Vec<String>,
    current_idx: usize,
}

#[derive(Debug, Deserialize)]
struct LayoutSwitched {
    idx: usize,
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
    let short_names = short_names().await;

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

    let mut layouts: Option<Layouts> = None;
    while let Some(line) = lines.next().await {
        let line = line.map_err(|error| Error::new(format!("failed to read event: {error}")))?;
        let Ok(event) = serde_json::from_str::<Event>(&line) else {
            continue;
        };

        if let Some(changed) = event.layouts_changed {
            layouts = Some(changed.keyboard_layouts);
        } else if let Some(switched) = event.layout_switched {
            let Some(layouts) = layouts.as_mut() else {
                continue;
            };
            layouts.current_idx = switched.idx;
        } else {
            continue;
        }

        if let Some(layouts) = layouts.as_ref() {
            forward(output, layouts, &short_names).await?;
        }
    }

    Err(Error::new("niri stopped sending events"))
}

async fn forward(
    output: &mut Sender<Update>,
    layouts: &Layouts,
    short_names: &HashMap<String, String>,
) -> Result<(), Error> {
    let Some(name) = layouts.names.get(layouts.current_idx) else {
        return Err(Error::new("keyboard layout index out of bounds"));
    };
    let short_name = short_names
        .get(name)
        .cloned()
        .unwrap_or_else(|| name.clone());

    output
        .send(Ok(Snapshot {
            name: name.clone(),
            short_name,
        }))
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
}
