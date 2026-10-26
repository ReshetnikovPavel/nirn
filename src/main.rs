use std::time::Duration;

use iced::widget::{column, row, text};
use iced::{Alignment, Element, Font, Length, Subscription, Task as Command, time};
use iced_layershell::application;
use iced_layershell::reexport::{Anchor, Layer};
use iced_layershell::settings::{LayerShellSettings, Settings, StartMode};
use iced_layershell::to_layer_message;

mod battery;
mod keyboard;
mod services;
pub mod theme;

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
    ])
}

struct Bar {
    clock: String,
    battery: String,
    keyboard_layout: String,
}

impl Default for Bar {
    fn default() -> Self {
        Self {
            clock: services::clock(),
            battery: String::new(),
            keyboard_layout: String::new(),
        }
    }
}

#[to_layer_message]
#[derive(Debug, Clone)]
enum Message {
    Tick1s,
    Battery(battery::Update),
    Keyboard(keyboard::Update),
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
        _ => unreachable!(),
    }
}

fn view(bar: &'_ Bar) -> Element<'_, Message> {
    let clock = text(&bar.clock).center();
    let keyboard_layout = text(&bar.keyboard_layout).center();
    let battery = text(&bar.battery).center();

    let left = column![].align_x(Alignment::Start).width(Length::Fill);

    let center = column![clock].align_x(Alignment::Center);

    let right = column![
        row![keyboard_layout, battery]
            .align_y(Alignment::Center)
            .spacing(8)
    ]
    .align_x(Alignment::End)
    .width(Length::Fill);
    row![left, center, right]
        .height(Length::Fill)
        .align_y(Alignment::Center)
        .into()
}

fn style(_counter: &Bar, theme: &iced::Theme) -> iced::theme::Style {
    use iced::theme::Style;
    Style {
        background_color: theme.palette().background.scale_alpha(0.9),
        text_color: theme.palette().text,
    }
}
