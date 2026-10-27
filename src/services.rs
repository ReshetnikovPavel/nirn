use chrono::{Datelike as _, Timelike as _};

use crate::{audio, battery, brightness, mpris, niri, system, wifi};

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
    format!("󰌌 {}", snapshot.short_name)
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

pub fn brightness(snapshot: Option<&brightness::Snapshot>) -> String {
    let Some(snapshot) = snapshot else {
        return String::new();
    };

    let icon = match snapshot.percentage {
        0..=33 => "󰃞",
        34..=66 => "󰃟",
        _ => "󰃠",
    };

    format!("{icon} {}%", snapshot.percentage)
}

pub fn audio(snapshot: Option<&audio::Snapshot>) -> String {
    let Some(snapshot) = snapshot else {
        return "󰝟".to_owned();
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

    format!("{headphones}{volume_icon} {}%", snapshot.volume)
}

pub fn microphone(snapshot: Option<&audio::Microphone>) -> String {
    let Some(snapshot) = snapshot else {
        return String::new();
    };

    let icon = if snapshot.muted { "" } else { "" };

    format!("{icon} {}%", snapshot.volume)
}

pub fn memory(snapshot: Option<&system::Snapshot>) -> String {
    let Some(snapshot) = snapshot else {
        return String::new();
    };

    let gigabytes = snapshot.memory_used_bytes as f64 / 1024.0 / 1024.0 / 1024.0;

    format!(" {gigabytes:.1}GB")
}

pub fn cpu(snapshot: Option<&system::Snapshot>) -> String {
    let Some(snapshot) = snapshot else {
        return String::new();
    };

    format!(" {:.0}%", snapshot.cpu_percentage)
}

pub fn mpris(snapshot: Option<&mpris::Snapshot>) -> String {
    snapshot
        .map(|snapshot| snapshot.track.clone())
        .unwrap_or_default()
}

pub fn battery(snapshot: &battery::Snapshot) -> String {
    let battery::Snapshot { status, percentage } = *snapshot;

    let charging = if matches!(status, battery::Status::Charging) {
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

    format!("{charging}{icon} {percentage:02}%")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_brightness_widget() {
        let snapshot = |percentage| brightness::Snapshot { percentage };

        assert_eq!(brightness(None), "");
        assert_eq!(brightness(Some(&snapshot(0))), "󰃝 0%");
        assert_eq!(brightness(Some(&snapshot(25))), "󰃝 25%");
        assert_eq!(brightness(Some(&snapshot(26))), "󰃞 26%");
        assert_eq!(brightness(Some(&snapshot(50))), "󰃞 50%");
        assert_eq!(brightness(Some(&snapshot(51))), "󰃟 51%");
        assert_eq!(brightness(Some(&snapshot(75))), "󰃟 75%");
        assert_eq!(brightness(Some(&snapshot(76))), "󰃠 76%");
        assert_eq!(brightness(Some(&snapshot(100))), "󰃠 100%");
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

        assert_eq!(microphone(None), "");
        assert_eq!(microphone(Some(&snapshot(98, false))), "󰍬 98%");
        assert_eq!(microphone(Some(&snapshot(98, true))), "󰍭 98%");
    }

    #[test]
    fn formats_system_widgets() {
        let snapshot = system::Snapshot {
            memory_used_bytes: 5_905_580_032,
            cpu_percentage: 12.4,
        };

        assert_eq!(memory(None), "");
        assert_eq!(cpu(None), "");
        assert_eq!(memory(Some(&snapshot)), "󰍛 5.5GB");
        assert_eq!(cpu(Some(&snapshot)), "󰻠 12%");
    }

    #[test]
    fn formats_audio_widget() {
        let snapshot = |volume, muted, headphones| audio::Snapshot {
            volume,
            muted,
            headphones,
            microphone: None,
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
