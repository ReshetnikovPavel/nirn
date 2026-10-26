use chrono::{Datelike as _, Timelike as _};

use crate::{battery, keyboard};

pub fn clock() -> String {
    let now = chrono::Local::now();
    let hour = now.hour();
    let minute = now.minute();
    let day = now.day();
    let month = chrono::Month::try_from(u8::try_from(now.month()).unwrap())
        .unwrap()
        .name();
    format!("{hour:02}:{minute:02} | {month} {day:02}")
}

pub fn keyboard_layout(snapshot: &keyboard::Snapshot) -> String {
    snapshot.short_name.clone()
}

pub fn battery(snapshot: &battery::Snapshot) -> String {
    let battery::Snapshot {
        status,
        percentage,
    } = *snapshot;

    let icon = if matches!(status, battery::Status::Charging) {
        match percentage {
            ..10 => "󰢟",
            ..20 => "󱊤",
            ..30 => "󰂆",
            ..40 => "󰂇",
            ..50 => "󰂈",
            ..60 => "󰢝",
            ..70 => "󰂉",
            ..80 => "󰢞",
            ..90 => "󰂊",
            ..100 => "󰂋",
            100 => "󰂅",
            _ => unreachable!(),
        }
    } else {
        match percentage {
            ..10 => "󰂎",
            ..20 => "󰁺",
            ..30 => "󰁻",
            ..40 => "󰁼",
            ..50 => "󰁽",
            ..60 => "󰁾",
            ..70 => "󰁿",
            ..80 => "󰂀",
            ..90 => "󰂁",
            ..100 => "󰂂",
            100 => "󰁹",
            _ => unreachable!(),
        }
    };

    format!("{percentage:02}% {icon}")
}
