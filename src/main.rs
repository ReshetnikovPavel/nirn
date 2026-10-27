use std::time::Duration;

use iced::widget::{column, container, mouse_area, row, space, stack, text};
use iced::{Alignment, Color, Element, Font, Length, Subscription, Task as Command, time};
use iced_layershell::application;
use iced_layershell::reexport::{Anchor, Layer};
use iced_layershell::settings::{LayerShellSettings, Settings, StartMode};
use iced_layershell::to_layer_message;
mod audio;
mod battery;
mod brightness;
mod marquee;
mod mpris;
mod niri;
mod services;
mod system;
pub mod theme;
mod wifi;

const HEIGHT: u32 = 35;

// Keep the track centered left of the clock. This is anchored to the bar
// center, so changing the number of workspaces never moves the track.
const TRACK_CENTER_OFFSET: f32 = marquee::WIDTH / 2.0 + 300.0;

pub fn main() -> Result<(), iced_layershell::Error> {
    application(|| Bar::default(), "Nirn", update, view)
        .theme(theme::rose_pine())
        .style(style)
        .subscription(subscription)
        .settings(Settings {
            default_font: Font::with_name("JetBrainsMono Nerd Font Mono"),
            layer_settings: LayerShellSettings {
                anchor: Anchor::Top | Anchor::Left | Anchor::Right,
                exclusive_zone: HEIGHT as i32,
                layer: Layer::Top,
                size: Some((0, HEIGHT)),
                start_mode: StartMode::Active,
                ..Default::default()
            },
            ..Default::default()
        })
        .run()
}

fn subscription(bar: &Bar) -> Subscription<Message> {
    let mut subscriptions = vec![
        time::every(Duration::from_secs(1)).map(|_| Message::Tick1s),
        Subscription::run(audio::updates).map(Message::Audio),
        Subscription::run(battery::updates).map(Message::Battery),
        Subscription::run(brightness::updates).map(Message::Brightness),
        Subscription::run(mpris::updates).map(Message::Mpris),
        Subscription::run(niri::updates).map(Message::Niri),
        Subscription::run(system::updates).map(Message::System),
        Subscription::run(wifi::updates).map(Message::Wifi),
    ];

    if bar.marquee.needs_animation() {
        subscriptions.push(time::every(marquee::TICK).map(|_| Message::MarqueeTick));
    }

    Subscription::batch(subscriptions)
}

struct Bar {
    clock: String,
    audio: String,
    microphone: String,
    battery: String,
    brightness: String,
    memory: String,
    cpu: String,
    keyboard_layout: String,
    wifi: String,
    track: String,
    marquee: marquee::State,
    workspaces: Vec<niri::Workspace>,
    hovered: bool,
}

impl Default for Bar {
    fn default() -> Self {
        Self {
            clock: services::clock(),
            audio: services::audio(None),
            microphone: services::microphone(None),
            battery: String::new(),
            brightness: services::brightness(None),
            memory: services::memory(None),
            cpu: services::cpu(None),
            keyboard_layout: String::new(),
            wifi: String::new(),
            track: String::new(),
            marquee: marquee::State::default(),
            workspaces: Vec::new(),
            hovered: false,
        }
    }
}

#[to_layer_message]
#[derive(Debug, Clone)]
enum Message {
    Tick1s,
    Audio(audio::Update),
    Battery(battery::Update),
    Brightness(brightness::Update),
    Mpris(mpris::Update),
    Niri(niri::Update),
    System(system::Update),
    Wifi(wifi::Update),
    MarqueeTick,
    MouseEnter,
    MouseExit,
}

fn update(bar: &mut Bar, message: Message) -> Command<Message> {
    match message {
        Message::Tick1s => {
            bar.clock = services::clock();
            Command::none()
        }
        Message::Audio(update) => {
            match update {
                Ok(snapshot) => {
                    bar.audio = services::audio(Some(&snapshot));
                    bar.microphone = services::microphone(snapshot.microphone.as_ref());
                }
                Err(error) => {
                    eprintln!("{}", error);
                    bar.audio = services::audio(None);
                    bar.microphone = services::microphone(None);
                }
            }
            Command::none()
        }
        Message::Battery(update) => {
            bar.battery = match update {
                Ok(Some(snapshot)) => services::battery(&snapshot),
                // No battery, e.g. on a desktop machine.
                Ok(None) => String::new(),
                Err(error) => {
                    eprintln!("{}", error);
                    "Err".to_owned()
                }
            };
            Command::none()
        }
        Message::Brightness(update) => {
            bar.brightness = match update {
                Ok(snapshot) => services::brightness(snapshot.as_ref()),
                Err(error) => {
                    eprintln!("{}", error);
                    "Err".to_owned()
                }
            };
            Command::none()
        }
        Message::Mpris(update) => {
            let track = match update {
                Ok(snapshot) => services::mpris(snapshot.as_ref()),
                Err(error) => {
                    eprintln!("{}", error);
                    String::new()
                }
            };

            if bar.track != track {
                bar.track = track;
                bar.marquee.set_track(&bar.track);
            }

            Command::none()
        }
        Message::Niri(update) => {
            match update {
                Ok(snapshot) => {
                    bar.workspaces = snapshot.workspaces;
                    bar.keyboard_layout = snapshot
                        .keyboard
                        .as_ref()
                        .map(services::keyboard_layout)
                        .unwrap_or_default();
                }
                Err(error) => {
                    eprintln!("{}", error);
                    bar.keyboard_layout = "Err".to_owned();
                    bar.workspaces.clear();
                }
            }
            Command::none()
        }
        Message::System(update) => {
            match update {
                Ok(snapshot) => {
                    bar.memory = services::memory(Some(&snapshot));
                    bar.cpu = services::cpu(Some(&snapshot));
                }
                Err(error) => {
                    eprintln!("{}", error);
                    bar.memory = String::new();
                    bar.cpu = String::new();
                }
            }
            Command::none()
        }
        Message::Wifi(update) => {
            bar.wifi = match update {
                Ok(snapshot) => services::wifi(snapshot.as_ref()),
                Err(error) => {
                    eprintln!("{}", error);
                    services::wifi(None)
                }
            };
            Command::none()
        }
        Message::MarqueeTick => {
            bar.marquee.tick();
            Command::none()
        }
        Message::MouseEnter => {
            bar.hovered = true;
            Command::none()
        }
        Message::MouseExit => {
            bar.hovered = false;
            Command::none()
        }
        _ => unreachable!(),
    }
}

fn view(bar: &'_ Bar) -> Element<'_, Message> {
    let clock = text(&bar.clock).center();
    let wifi = text(&bar.wifi).center();
    let audio = text(&bar.audio).center();
    let microphone = text(&bar.microphone).center();
    let brightness = text(&bar.brightness).center();
    let memory = text(&bar.memory).center();
    let cpu = text(&bar.cpu).center();
    let keyboard_layout = text(&bar.keyboard_layout).center();
    let battery = text(&bar.battery).center();

    let workspaces = row(bar.workspaces.iter().map(workspace_view)).spacing(14);
    let left = column![workspaces]
        .align_x(Alignment::Start)
        .width(Length::Fill);

    let center = column![clock].align_x(Alignment::Center);

    let right = column![
        row![
            cpu,
            memory,
            audio,
            microphone,
            brightness,
            wifi,
            keyboard_layout,
            battery
        ]
        .align_y(Alignment::Center)
        .spacing(12)
    ]
    .align_x(Alignment::End)
    .width(Length::Fill);
    let content = row![left, center, right]
        .height(Length::Fill)
        .align_y(Alignment::Center);

    let hovered = bar.hovered;

    let mut layers: Vec<Element<'_, Message>> = vec![
        container(content)
            .padding(iced::Padding {
                top: 4.0,
                right: 14.0,
                bottom: 0.0,
                left: 14.0,
            })
            .width(Length::Fill)
            .height(Length::Fill)
            .style(bar_style)
            .into(),
    ];

    if !bar.track.is_empty() {
        layers.push(
            container(marquee::widget(&bar.track, &bar.marquee))
                .padding(iced::Padding {
                    top: 4.0,
                    right: 14.0 + 2.0 * TRACK_CENTER_OFFSET,
                    bottom: 0.0,
                    left: 14.0,
                })
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center)
                .into(),
        );
    }

    layers.push(
        container(space())
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |theme| hover_border_style(theme, hovered))
            .into(),
    );

    let bar = stack(layers).width(Length::Fill).height(Length::Fill);

    // The border container is taller than the bar and shifted up so its top
    // edge is clipped by the layer surface.
    let bar = container(bar)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(iced::Padding {
            top: -4.0,
            right: 0.0,
            bottom: 0.0,
            left: 0.0,
        });

    mouse_area(bar)
        .on_enter(Message::MouseEnter)
        .on_exit(Message::MouseExit)
        .into()
}

fn bar_style(theme: &iced::Theme) -> iced::widget::container::Style {
    use iced::widget::container;

    container::Style {
        background: Some(theme.palette().background.scale_alpha(0.6).into()),
        border: iced::border::Border {
            radius: iced::border::bottom(12.0),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn hover_border_style(theme: &iced::Theme, hovered: bool) -> iced::widget::container::Style {
    use iced::widget::container;

    container::Style {
        border: iced::border::Border {
            color: if hovered {
                theme.palette().primary
            } else {
                Color::TRANSPARENT
            },
            width: if hovered { 4.0 } else { 0.0 },
            radius: iced::border::bottom(12.0),
        },
        ..Default::default()
    }
}

fn workspace_view(workspace: &niri::Workspace) -> Element<'_, Message> {
    text(services::workspace_label(workspace))
        .color(workspace_color(workspace))
        .into()
}

fn workspace_color(workspace: &niri::Workspace) -> Color {
    let palette = theme::RosePine::MAIN;

    if workspace.is_urgent {
        palette.love
    } else if workspace.is_focused || workspace.is_active {
        palette.text
    } else {
        palette.muted
    }
}

fn style(_counter: &Bar, theme: &iced::Theme) -> iced::theme::Style {
    use iced::theme::Style;
    Style {
        background_color: Color::TRANSPARENT,
        text_color: theme.palette().text,
    }
}
