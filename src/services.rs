use chrono::{Datelike as _, Timelike as _};

use crate::{audio, battery, brightness, mpris, niri, system, wifi};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Status {
    pub icon: String,
    pub text: String,
}

impl Status {
    pub fn new(icon: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            icon: icon.into(),
            text: text.into(),
        }
    }

    pub fn text(text: impl Into<String>) -> Self {
        Self::new("", text)
    }
}

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

pub fn keyboard_layout(snapshot: &niri::Keyboard) -> Status {
    Status::new("󰌌", snapshot.short_name.clone())
}

pub fn workspace_label(workspace: &niri::Workspace) -> String {
    workspace
        .name
        .clone()
        .unwrap_or_else(|| workspace.idx.to_string())
}

pub fn wifi(snapshot: Option<&wifi::Snapshot>) -> Status {
    let Some(snapshot) = snapshot else {
        return Status::new("󰤮", "");
    };

    let icon = match snapshot.strength {
        0..=20 => "󰤯",
        21..=40 => "󰤟",
        41..=60 => "󰤢",
        61..=80 => "󰤥",
        _ => "󰤨",
    };

    Status::new(icon, snapshot.ssid.clone())
}

pub fn brightness(snapshot: Option<&brightness::Snapshot>) -> Status {
    let Some(snapshot) = snapshot else {
        return Status::default();
    };

    let icon = match snapshot.percentage {
        0..=33 => "󰃞",
        34..=66 => "󰃟",
        _ => "󰃠",
    };

    Status::new(icon, format!("{}%", snapshot.percentage))
}

pub fn audio(snapshot: Option<&audio::Snapshot>) -> Status {
    let Some(snapshot) = snapshot else {
        return Status::new("󰝟", "");
    };

    let volume_icon = if snapshot.muted {
        "󰝟"
    } else {
        match snapshot.volume {
            0..=25 => "",
            26..=50 => "",
            51..=75 => "",
            _ => "",
        }
    };

    let headphones = if snapshot.headphones { "󰋋 " } else { "" };
    let icon = format!("{headphones}{volume_icon}");

    Status::new(icon, format!("{}%", snapshot.volume))
}

pub fn microphone(snapshot: Option<&audio::Microphone>) -> Status {
    let Some(snapshot) = snapshot else {
        return Status::default();
    };

    let icon = if snapshot.muted { "" } else { "" };

    Status::new(icon, format!("{}%", snapshot.volume))
}

pub fn memory(snapshot: Option<&system::Snapshot>) -> Status {
    let Some(snapshot) = snapshot else {
        return Status::default();
    };

    let gigabytes = snapshot.memory_used_bytes as f64 / 1024.0 / 1024.0 / 1024.0;

    Status::new("", format!("{gigabytes:.1}GB"))
}

pub fn cpu(snapshot: Option<&system::Snapshot>) -> Status {
    let Some(snapshot) = snapshot else {
        return Status::default();
    };

    Status::new("", format!("{:.0}%", snapshot.cpu_percentage))
}

pub fn mpris(snapshot: Option<&mpris::Snapshot>) -> String {
    snapshot
        .map(|snapshot| snapshot.track.clone())
        .unwrap_or_default()
}

pub fn battery(snapshot: &battery::Snapshot) -> Status {
    let battery::Snapshot { status, percentage } = *snapshot;

    let charging = if matches!(status, battery::Status::Charging | battery::Status::Full) {
        "󱐋"
    } else {
        ""
    };
    let icon = match percentage {
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
    };

    Status::new(format!("{charging}{icon}"), format!("{percentage:02}%"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_brightness_widget() {
        let snapshot = |percentage| brightness::Snapshot { percentage };

        assert_eq!(brightness(None), Status::default());
        assert_eq!(brightness(Some(&snapshot(0))), Status::new("󰃞", "0%"));
        assert_eq!(brightness(Some(&snapshot(33))), Status::new("󰃞", "33%"));
        assert_eq!(brightness(Some(&snapshot(34))), Status::new("󰃟", "34%"));
        assert_eq!(brightness(Some(&snapshot(66))), Status::new("󰃟", "66%"));
        assert_eq!(brightness(Some(&snapshot(67))), Status::new("󰃠", "67%"));
        assert_eq!(brightness(Some(&snapshot(100))), Status::new("󰃠", "100%"));
    }

    #[test]
    fn formats_wifi_widget() {
        let snapshot = |strength| wifi::Snapshot {
            ssid: "pavelresh".to_owned(),
            strength,
        };

        assert_eq!(wifi(None), Status::new("󰤮", ""));
        assert_eq!(wifi(Some(&snapshot(0))), Status::new("󰤯", "pavelresh"));
        assert_eq!(wifi(Some(&snapshot(20))), Status::new("󰤯", "pavelresh"));
        assert_eq!(wifi(Some(&snapshot(21))), Status::new("󰤟", "pavelresh"));
        assert_eq!(wifi(Some(&snapshot(40))), Status::new("󰤟", "pavelresh"));
        assert_eq!(wifi(Some(&snapshot(41))), Status::new("󰤢", "pavelresh"));
        assert_eq!(wifi(Some(&snapshot(60))), Status::new("󰤢", "pavelresh"));
        assert_eq!(wifi(Some(&snapshot(61))), Status::new("󰤥", "pavelresh"));
        assert_eq!(wifi(Some(&snapshot(80))), Status::new("󰤥", "pavelresh"));
        assert_eq!(wifi(Some(&snapshot(81))), Status::new("󰤨", "pavelresh"));
        assert_eq!(wifi(Some(&snapshot(100))), Status::new("󰤨", "pavelresh"));
    }

    #[test]
    fn formats_keyboard_layout_widget() {
        let snapshot = niri::Keyboard {
            name: "English (US)".to_owned(),
            short_name: "us".to_owned(),
        };

        assert_eq!(keyboard_layout(&snapshot), Status::new("󰌌", "us"));
    }

    #[test]
    fn formats_mpris_widget() {
        let snapshot = mpris::Snapshot {
            track: "Artist – Track".to_owned(),
        };

        assert_eq!(mpris(None), "");
        assert_eq!(mpris(Some(&snapshot)), "Artist – Track");
    }

    #[test]
    fn formats_microphone_widget() {
        let snapshot = |volume, muted| audio::Microphone { volume, muted };

        assert_eq!(microphone(None), Status::default());
        assert_eq!(
            microphone(Some(&snapshot(98, false))),
            Status::new("", "98%")
        );
        assert_eq!(
            microphone(Some(&snapshot(98, true))),
            Status::new("", "98%")
        );
    }

    #[test]
    fn formats_system_widgets() {
        let snapshot = system::Snapshot {
            memory_used_bytes: 5_905_580_032,
            cpu_percentage: 12.4,
        };

        assert_eq!(memory(None), Status::default());
        assert_eq!(cpu(None), Status::default());
        assert_eq!(memory(Some(&snapshot)), Status::new("", "5.5GB"));
        assert_eq!(cpu(Some(&snapshot)), Status::new("", "12%"));
    }

    #[test]
    fn formats_battery_widget() {
        let snapshot = |status, percentage| battery::Snapshot { status, percentage };

        assert_eq!(
            battery(&snapshot(battery::Status::Discharging, 100)),
            Status::new("󰁹", "100%")
        );
        assert_eq!(
            battery(&snapshot(battery::Status::Charging, 100)),
            Status::new("󱐋󰁹", "100%")
        );
        assert_eq!(
            battery(&snapshot(battery::Status::Full, 100)),
            Status::new("󱐋󰁹", "100%")
        );
        assert_eq!(
            battery(&snapshot(battery::Status::NotCharging, 42)),
            Status::new("󰁽", "42%")
        );
        assert_eq!(
            battery(&snapshot(battery::Status::Charging, 9)),
            Status::new("󱐋󰂎", "09%")
        );
    }

    #[test]
    fn formats_audio_widget() {
        let snapshot = |volume, muted, headphones| audio::Snapshot {
            volume,
            muted,
            headphones,
            microphone: None,
        };

        assert_eq!(
            audio(Some(&snapshot(0, false, false))),
            Status::new("", "0%")
        );
        assert_eq!(
            audio(Some(&snapshot(25, false, false))),
            Status::new("", "25%")
        );
        assert_eq!(
            audio(Some(&snapshot(26, false, false))),
            Status::new("", "26%")
        );
        assert_eq!(
            audio(Some(&snapshot(50, false, false))),
            Status::new("", "50%")
        );
        assert_eq!(
            audio(Some(&snapshot(51, false, false))),
            Status::new("", "51%")
        );
        assert_eq!(
            audio(Some(&snapshot(75, false, false))),
            Status::new("", "75%")
        );
        assert_eq!(
            audio(Some(&snapshot(76, false, false))),
            Status::new("", "76%")
        );
        assert_eq!(
            audio(Some(&snapshot(100, false, false))),
            Status::new("", "100%")
        );
        assert_eq!(
            audio(Some(&snapshot(45, true, false))),
            Status::new("󰝟", "45%")
        );
        assert_eq!(
            audio(Some(&snapshot(45, false, true))),
            Status::new("󰋋 ", "45%")
        );
        assert_eq!(
            audio(Some(&snapshot(45, true, true))),
            Status::new("󰋋 󰝟", "45%")
        );
        assert_eq!(audio(None), Status::new("󰝟", ""));
    }
}
