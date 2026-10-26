use chrono::{Datelike as _, Timelike as _};

use crate::{audio, battery, niri, wifi};

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

pub fn keyboard_layout(snapshot: &niri::Keyboard) -> String {
    snapshot.short_name.clone()
}

pub fn workspace_label(workspace: &niri::Workspace) -> String {
    workspace
        .name
        .clone()
        .unwrap_or_else(|| workspace.idx.to_string())
}

pub fn wifi(snapshot: Option<&wifi::Snapshot>) -> String {
    match snapshot {
        Some(snapshot) => format!("󰖩 {}", snapshot.ssid),
        None => "󰖪".to_owned(),
    }
}

pub fn audio(snapshot: Option<&audio::Snapshot>) -> String {
    let Some(snapshot) = snapshot else {
        return "󰝟".to_owned();
    };

    let volume_icon = if snapshot.muted {
        "󰝟"
    } else {
        match snapshot.volume {
            0..=33 => "󰕿",
            34..=66 => "󰖀",
            _ => "󰕾",
        }
    };

    let headphones = if snapshot.headphones { "󰋋 " } else { "" };

    format!("{headphones}{volume_icon} {}%", snapshot.volume)
}

pub fn battery(snapshot: &battery::Snapshot) -> String {
    let battery::Snapshot { status, percentage } = *snapshot;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_audio_widget() {
        let snapshot = |volume, muted, headphones| audio::Snapshot {
            volume,
            muted,
            headphones,
        };

        assert_eq!(audio(Some(&snapshot(0, false, false))), "󰕿 0%");
        assert_eq!(audio(Some(&snapshot(33, false, false))), "󰕿 33%");
        assert_eq!(audio(Some(&snapshot(34, false, false))), "󰖀 34%");
        assert_eq!(audio(Some(&snapshot(66, false, false))), "󰖀 66%");
        assert_eq!(audio(Some(&snapshot(67, false, false))), "󰕾 67%");
        assert_eq!(audio(Some(&snapshot(100, false, false))), "󰕾 100%");
        assert_eq!(audio(Some(&snapshot(45, true, false))), "󰝟 45%");
        assert_eq!(audio(Some(&snapshot(45, false, true))), "󰋋 󰖀 45%");
        assert_eq!(audio(Some(&snapshot(45, true, true))), "󰋋 󰝟 45%");
        assert_eq!(audio(None), "󰝟");
    }
}
