use std::time::Duration;

use iced::widget::{column, container, mouse_area, row, space, stack, text};
use iced::{Alignment, Color, Element, Font, Length, Subscription, Task as Command, time};
use iced_layershell::application;
use iced_layershell::reexport::{Anchor, Layer};
use iced_layershell::settings::{LayerShellSettings, Settings, StartMode};
use iced_layershell::to_layer_message;
mod battery;
mod keyboard;
mod services;
pub mod theme;
mod workspaces;

const HEIGHT: u32 = 35;

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

fn subscription(_: &Bar) -> Subscription<Message> {
    Subscription::batch([
        time::every(Duration::from_secs(1)).map(|_| Message::Tick1s),
        Subscription::run(battery::updates).map(Message::Battery),
        Subscription::run(keyboard::updates).map(Message::Keyboard),
        Subscription::run(workspaces::updates).map(Message::Workspaces),
    ])
}

struct Bar {
    clock: String,
    battery: String,
    keyboard_layout: String,
    workspaces: Vec<workspaces::Workspace>,
    hovered: bool,
}

impl Default for Bar {
    fn default() -> Self {
        Self {
            clock: services::clock(),
            battery: String::new(),
            keyboard_layout: String::new(),
            workspaces: Vec::new(),
            hovered: false,
        }
    }
}

#[to_layer_message]
#[derive(Debug, Clone)]
enum Message {
    Tick1s,
    Battery(battery::Update),
    Keyboard(keyboard::Update),
    Workspaces(workspaces::Update),
    MouseEnter,
    MouseExit,
}

fn update(bar: &mut Bar, message: Message) -> Command<Message> {
    match message {
        Message::Tick1s => {
            bar.clock = services::clock();
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
        Message::Keyboard(update) => {
            bar.keyboard_layout = match update {
                Ok(snapshot) => services::keyboard_layout(&snapshot),
                Err(error) => {
                    eprintln!("{}", error);
                    "Err".to_owned()
                }
            };
            Command::none()
        }
        Message::Workspaces(update) => {
            bar.workspaces = match update {
                Ok(snapshot) => snapshot.workspaces,
                Err(error) => {
                    eprintln!("{}", error);
                    Vec::new()
                }
            };
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
    let keyboard_layout = text(&bar.keyboard_layout).center();
    let battery = text(&bar.battery).center();

    let workspaces = row(bar.workspaces.iter().map(workspace_view)).spacing(14);
    let left = column![workspaces]
        .align_x(Alignment::Start)
        .width(Length::Fill);

    let center = column![clock].align_x(Alignment::Center);

    let right = column![
        row![keyboard_layout, battery]
            .align_y(Alignment::Center)
            .spacing(12)
    ]
    .align_x(Alignment::End)
    .width(Length::Fill);
    let content = row![left, center, right]
        .height(Length::Fill)
        .align_y(Alignment::Center);

    let hovered = bar.hovered;

    let bar = stack![
        container(content)
            .padding(iced::Padding {
                top: 4.0,
                right: 14.0,
                bottom: 0.0,
                left: 14.0,
            })
            .width(Length::Fill)
            .height(Length::Fill)
            .style(bar_style),
        container(space())
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |theme| hover_border_style(theme, hovered)),
    ]
    .width(Length::Fill)
    .height(Length::Fill);

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

fn workspace_view(workspace: &workspaces::Workspace) -> Element<'_, Message> {
    text(services::workspace_label(workspace))
        .color(workspace_color(workspace))
        .into()
}

fn workspace_color(workspace: &workspaces::Workspace) -> Color {
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
