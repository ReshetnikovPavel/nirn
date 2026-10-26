use std::fmt::{self, Display};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use async_io::Timer;
use iced::futures::channel::mpsc::{self, Sender};
use iced::futures::sink::SinkExt;
use iced::futures::{Stream, StreamExt};
use iced::stream;
use libpulse_binding as pulse;
use pulse::callbacks::ListResult;
use pulse::context::subscribe::InterestMaskSet;
use pulse::context::{Context, FlagSet as ContextFlagSet, State as ContextState};
use pulse::def::{DevicePortType, PortAvailable};
use pulse::mainloop::standard::{IterateResult, Mainloop};
use pulse::operation::{Operation, State as OperationState};
use pulse::time::MicroSeconds;
use pulse::volume::ChannelVolumes;

const RETRY_DELAY: Duration = Duration::from_secs(5);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const OPERATION_TIMEOUT: Duration = Duration::from_secs(2);
// PulseAudio events wake the poll immediately; this is only a fallback for missed events.
const EVENT_TIMEOUT: MicroSeconds = MicroSeconds(1_000_000);
const VOLUME_NORM: u64 = 65_536;

pub type Update = Result<Snapshot, Error>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub volume: u8,
    pub muted: bool,
    pub headphones: bool,
    pub microphone: Option<Microphone>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Microphone {
    pub volume: u8,
    pub muted: bool,
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
        write!(f, "audio: {}", self.0)
    }
}

impl std::error::Error for Error {}

pub fn updates() -> impl Stream<Item = Update> {
    stream::channel(4, async |mut output| {
        let mut delay = Duration::ZERO;

        loop {
            if !delay.is_zero() {
                Timer::after(delay).await;
            }

            match watch(&mut output).await {
                Ok(()) => {
                    delay = RETRY_DELAY;
                }
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
    let (tx, mut rx) = mpsc::unbounded();

    let handle = thread::Builder::new()
        .name("pulseaudio".to_owned())
        .spawn(move || {
            if let Err(error) = run(tx.clone()) {
                let _ = tx.unbounded_send(Err(error));
            }
        })
        .map_err(|error| Error::new(format!("failed to start PulseAudio thread: {error}")))?;

    while let Some(update) = rx.next().await {
        output
            .send(update)
            .await
            .map_err(|_| Error::new("update channel closed"))?;
    }

    if handle.join().is_err() {
        return Err(Error::new("PulseAudio thread panicked"));
    }

    Ok(())
}

fn run(tx: mpsc::UnboundedSender<Update>) -> Result<(), Error> {
    let mut mainloop =
        Mainloop::new().ok_or_else(|| Error::new("failed to create PulseAudio mainloop"))?;
    let mut context = Context::new(&mainloop, "nirn")
        .ok_or_else(|| Error::new("failed to create PulseAudio context"))?;

    context
        .connect(None, ContextFlagSet::NOFLAGS, None)
        .map_err(|error| Error::new(format!("failed to connect to PulseAudio: {error}")))?;

    wait_until_ready(&mut mainloop, &context)?;

    subscribe(&mut mainloop, &mut context)?;

    let mut last = None;
    forward_if_changed(&tx, &mut last, query_state(&context, &mut mainloop)?)?;

    loop {
        wait_for_event(&mut mainloop)?;

        match context.get_state() {
            ContextState::Failed => return Err(Error::new("PulseAudio connection failed")),
            ContextState::Terminated => return Err(Error::new("PulseAudio connection terminated")),
            _ => {}
        }

        let snapshot = query_state(&context, &mut mainloop)?;
        forward_if_changed(&tx, &mut last, snapshot)?;
    }
}

fn wait_for_event(mainloop: &mut Mainloop) -> Result<(), Error> {
    mainloop
        .prepare(Some(EVENT_TIMEOUT))
        .map_err(|error| Error::new(format!("PulseAudio prepare failed: {error}")))?;
    mainloop
        .poll()
        .map_err(|error| Error::new(format!("PulseAudio poll failed: {error}")))?;
    mainloop
        .dispatch()
        .map_err(|error| Error::new(format!("PulseAudio dispatch failed: {error}")))?;

    Ok(())
}

fn wait_until_ready(mainloop: &mut Mainloop, context: &Context) -> Result<(), Error> {
    let deadline = Instant::now() + CONNECT_TIMEOUT;

    loop {
        match mainloop.iterate(false) {
            IterateResult::Quit(_) => return Err(Error::new("PulseAudio mainloop quit")),
            IterateResult::Err(error) => {
                return Err(Error::new(format!("PulseAudio mainloop error: {error}")));
            }
            IterateResult::Success(_) => {}
        }

        match context.get_state() {
            ContextState::Ready => return Ok(()),
            ContextState::Failed => return Err(Error::new("PulseAudio connection failed")),
            ContextState::Terminated => return Err(Error::new("PulseAudio connection terminated")),
            _ => {}
        }

        if Instant::now() >= deadline {
            return Err(Error::new("PulseAudio connection timed out"));
        }

        thread::sleep(Duration::from_millis(2));
    }
}

fn subscribe(mainloop: &mut Mainloop, context: &mut Context) -> Result<(), Error> {
    let (tx, rx) = std::sync::mpsc::channel();
    let mask = InterestMaskSet::SINK
        | InterestMaskSet::SOURCE
        | InterestMaskSet::CARD
        | InterestMaskSet::SERVER;
    let operation = context.subscribe(mask, move |success| {
        let _ = tx.send(success);
    });

    wait_for_done(mainloop, &operation)?;

    let success = rx
        .try_recv()
        .map_err(|_| Error::new("PulseAudio subscription callback missing"))?;
    if !success {
        return Err(Error::new("PulseAudio subscription failed"));
    }

    Ok(())
}

fn wait_for_done<T: ?Sized>(
    mainloop: &mut Mainloop,
    operation: &Operation<T>,
) -> Result<(), Error> {
    let deadline = Instant::now() + OPERATION_TIMEOUT;

    loop {
        match operation.get_state() {
            OperationState::Done => return Ok(()),
            OperationState::Cancelled => return Err(Error::new("PulseAudio operation cancelled")),
            OperationState::Running => {}
        }

        if Instant::now() >= deadline {
            return Err(Error::new("PulseAudio operation timed out"));
        }

        match mainloop.iterate(false) {
            IterateResult::Quit(_) => return Err(Error::new("PulseAudio mainloop quit")),
            IterateResult::Err(error) => {
                return Err(Error::new(format!("PulseAudio mainloop error: {error}")));
            }
            IterateResult::Success(_) => {}
        }

        thread::sleep(Duration::from_millis(1));
    }
}

#[derive(Default)]
struct QueryState {
    default_sink: Option<String>,
    default_source: Option<String>,
    volume: u8,
    muted: bool,
    headphones: bool,
    sink_found: bool,
    microphone_volume: u8,
    microphone_muted: bool,
    microphone_found: bool,
    failed: bool,
}

fn query_state(context: &Context, mainloop: &mut Mainloop) -> Result<Snapshot, Error> {
    let state = Arc::new(Mutex::new(QueryState::default()));

    {
        let state_cb = Arc::clone(&state);
        let operation = context.introspect().get_server_info(move |info| {
            let mut state = state_cb.lock().unwrap();
            state.default_sink = info.default_sink_name.as_ref().map(|name| name.to_string());
            state.default_source = info
                .default_source_name
                .as_ref()
                .map(|name| name.to_string());
        });
        wait_for_done(mainloop, &operation)?;
    }

    {
        let state_cb = Arc::clone(&state);
        let operation = context
            .introspect()
            .get_sink_info_list(move |list| match list {
                ListResult::Item(sink) => {
                    let mut state = state_cb.lock().unwrap();
                    if state.default_sink.as_deref() == sink.name.as_deref() {
                        state.volume = volume_percent(&sink.volume);
                        state.muted = sink.mute;
                        state.sink_found = true;
                    }
                    if sink_has_headphones(sink) {
                        state.headphones = true;
                    }
                }
                ListResult::End => {}
                ListResult::Error => {
                    state_cb.lock().unwrap().failed = true;
                }
            });
        wait_for_done(mainloop, &operation)?;
    }

    {
        let state_cb = Arc::clone(&state);
        let operation = context
            .introspect()
            .get_source_info_list(move |list| match list {
                ListResult::Item(source) => {
                    let mut state = state_cb.lock().unwrap();
                    if state.default_source.as_deref() == source.name.as_deref() {
                        state.microphone_volume = volume_percent(&source.volume);
                        state.microphone_muted = source.mute;
                        state.microphone_found = true;
                    }
                }
                ListResult::End => {}
                ListResult::Error => {
                    state_cb.lock().unwrap().failed = true;
                }
            });
        wait_for_done(mainloop, &operation)?;
    }

    {
        let state_cb = Arc::clone(&state);
        let operation = context
            .introspect()
            .get_card_info_list(move |list| match list {
                ListResult::Item(card) => {
                    if card_has_headphones(card) {
                        state_cb.lock().unwrap().headphones = true;
                    }
                }
                ListResult::End => {}
                ListResult::Error => {
                    state_cb.lock().unwrap().failed = true;
                }
            });
        wait_for_done(mainloop, &operation)?;
    }

    let state = state.lock().unwrap();
    if state.failed {
        return Err(Error::new("PulseAudio failed to read audio state"));
    }

    Ok(Snapshot {
        volume: if state.sink_found { state.volume } else { 0 },
        muted: if state.sink_found { state.muted } else { true },
        headphones: state.headphones,
        microphone: state.microphone_found.then_some(Microphone {
            volume: state.microphone_volume,
            muted: state.microphone_muted,
        }),
    })
}

fn forward_if_changed(
    tx: &mpsc::UnboundedSender<Update>,
    last: &mut Option<Snapshot>,
    snapshot: Snapshot,
) -> Result<(), Error> {
    if last.as_ref() == Some(&snapshot) {
        return Ok(());
    }

    *last = Some(snapshot.clone());
    tx.unbounded_send(Ok(snapshot))
        .map_err(|_| Error::new("update channel closed"))
}

fn volume_percent(volumes: &ChannelVolumes) -> u8 {
    let values = volumes.get();
    if values.is_empty() {
        return 0;
    }

    let total: u64 = values.iter().map(|volume| u64::from(volume.0)).sum();
    let average = total / values.len() as u64;

    ((average * 100 + VOLUME_NORM / 2) / VOLUME_NORM).min(255) as u8
}

fn sink_has_headphones(sink: &pulse::context::introspect::SinkInfo<'_>) -> bool {
    if sink
        .active_port
        .as_ref()
        .is_some_and(|port| is_headphone_type(port.r#type))
    {
        return true;
    }

    if sink
        .ports
        .iter()
        .any(|port| is_headphone_type(port.r#type) && is_port_available(port.available))
    {
        return true;
    }

    [
        "device.icon_name",
        "device.form_factor",
        "device.description",
    ]
    .iter()
    .filter_map(|key| sink.proplist.get_str(key))
    .any(|value| {
        let value = value.to_ascii_lowercase();
        value.contains("headphone") || value.contains("headset")
    })
}

fn card_has_headphones(card: &pulse::context::introspect::CardInfo<'_>) -> bool {
    card.ports
        .iter()
        .any(|port| is_headphone_type(port.r#type) && is_port_available(port.available))
}

fn is_headphone_type(kind: DevicePortType) -> bool {
    matches!(kind, DevicePortType::Headphones | DevicePortType::Headset)
}

fn is_port_available(available: PortAvailable) -> bool {
    matches!(available, PortAvailable::Yes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn volumes(values: &[u32]) -> ChannelVolumes {
        let mut volumes = ChannelVolumes::default();
        if values.is_empty() {
            return volumes;
        }

        volumes.set(values.len() as u8, pulse::volume::Volume(0));
        for (channel, value) in volumes.get_mut().iter_mut().zip(values) {
            *channel = pulse::volume::Volume(*value);
        }
        volumes
    }

    #[test]
    fn computes_average_volume() {
        assert_eq!(volume_percent(&volumes(&[0, 0])), 0);
        assert_eq!(volume_percent(&volumes(&[32_768, 32_768])), 50);
        assert_eq!(volume_percent(&volumes(&[65_536, 65_536])), 100);
        assert_eq!(volume_percent(&volumes(&[98_304, 98_304])), 150);
        assert_eq!(volume_percent(&volumes(&[0, 65_536])), 50);
        assert_eq!(volume_percent(&volumes(&[196_608, 196_608])), 255);
        assert_eq!(volume_percent(&volumes(&[])), 0);
    }

    #[test]
    fn detects_headphone_port_types() {
        assert!(is_headphone_type(DevicePortType::Headphones));
        assert!(is_headphone_type(DevicePortType::Headset));
        assert!(!is_headphone_type(DevicePortType::Speaker));
    }

    #[test]
    fn detects_available_ports() {
        assert!(is_port_available(PortAvailable::Yes));
        assert!(!is_port_available(PortAvailable::No));
        assert!(!is_port_available(PortAvailable::Unknown));
    }
}
