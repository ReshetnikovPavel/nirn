use std::fmt::{self, Display};
use std::time::Duration;

use async_io::Timer;
use iced::futures::Stream;
use iced::futures::sink::SinkExt;
use iced::stream;

const SAMPLE_INTERVAL: Duration = Duration::from_secs(1);
const RETRY_DELAY: Duration = Duration::from_secs(5);

pub type Update = Result<Snapshot, Error>;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Snapshot {
    pub memory_used_bytes: u64,
    pub cpu_percentage: f32,
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
        write!(f, "system: {}", self.0)
    }
}

impl std::error::Error for Error {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CpuTimes {
    total: u64,
    idle: u64,
}

pub fn updates() -> impl Stream<Item = Update> {
    stream::channel(4, async |mut output| {
        let mut previous = match read_cpu_times().await {
            Ok(times) => times,
            Err(error) => {
                let _ = output.send(Err(error)).await;
                return;
            }
        };

        loop {
            Timer::after(SAMPLE_INTERVAL).await;

            let current = match read_cpu_times().await {
                Ok(times) => times,
                Err(error) => {
                    if output.send(Err(error)).await.is_err() {
                        break;
                    }
                    Timer::after(RETRY_DELAY).await;
                    continue;
                }
            };

            let cpu_percentage = cpu_usage(previous, current);
            previous = current;

            let memory_used_bytes = match read_memory_used().await {
                Ok(bytes) => bytes,
                Err(error) => {
                    if output.send(Err(error)).await.is_err() {
                        break;
                    }
                    Timer::after(RETRY_DELAY).await;
                    continue;
                }
            };

            let snapshot = Snapshot {
                memory_used_bytes,
                cpu_percentage,
            };

            if output.send(Ok(snapshot)).await.is_err() {
                break;
            }
        }
    })
}

async fn read_cpu_times() -> Result<CpuTimes, Error> {
    let contents = smol::fs::read_to_string("/proc/stat")
        .await
        .map_err(|error| Error::new(format!("failed to read /proc/stat: {error}")))?;

    parse_cpu_times(&contents).ok_or_else(|| Error::new("failed to parse /proc/stat"))
}

async fn read_memory_used() -> Result<u64, Error> {
    let contents = smol::fs::read_to_string("/proc/meminfo")
        .await
        .map_err(|error| Error::new(format!("failed to read /proc/meminfo: {error}")))?;

    parse_memory_used(&contents).ok_or_else(|| Error::new("failed to parse /proc/meminfo"))
}

fn parse_cpu_times(contents: &str) -> Option<CpuTimes> {
    let line = contents.lines().next()?;
    let mut fields = line.split_whitespace();

    if fields.next()? != "cpu" {
        return None;
    }

    let values: Vec<u64> = fields
        .take(8)
        .map(|field| field.parse().ok())
        .collect::<Option<Vec<_>>>()?;

    if values.len() < 4 {
        return None;
    }

    Some(CpuTimes {
        total: values.iter().sum(),
        idle: values[3] + values.get(4).copied().unwrap_or(0),
    })
}

fn parse_memory_used(contents: &str) -> Option<u64> {
    let mut total = None;
    let mut available = None;

    for line in contents.lines() {
        let mut fields = line.split_whitespace();

        let Some(name) = fields.next() else {
            continue;
        };
        let Some(value) = fields.next().and_then(|value| value.parse::<u64>().ok()) else {
            continue;
        };

        match name {
            "MemTotal:" => total = Some(value),
            "MemAvailable:" => available = Some(value),
            _ => {}
        }
    }

    let total = total?;
    let available = available?;

    Some(total.saturating_sub(available).saturating_mul(1024))
}

fn cpu_usage(previous: CpuTimes, current: CpuTimes) -> f32 {
    let total = current.total.saturating_sub(previous.total);
    if total == 0 {
        return 0.0;
    }

    let idle = current.idle.saturating_sub(previous.idle);
    let busy = total.saturating_sub(idle);

    (busy as f32 / total as f32 * 100.0).clamp(0.0, 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_memory_used() {
        let meminfo = "\
MemTotal:       16384000 kB
MemFree:         1000000 kB
MemAvailable:    6291456 kB
Buffers:          200000 kB
";

        assert_eq!(parse_memory_used(meminfo), Some(10_092_544 * 1024));
    }

    #[test]
    fn parses_cpu_times() {
        let stat = "cpu  100 0 50 800 50 0 0 0 0 0\ncpu0 50 0 25 400 25 0 0 0 0 0\n";

        assert_eq!(
            parse_cpu_times(stat),
            Some(CpuTimes {
                total: 1000,
                idle: 850,
            })
        );
    }

    #[test]
    fn computes_cpu_usage_from_deltas() {
        let previous = CpuTimes {
            total: 1000,
            idle: 800,
        };
        let current = CpuTimes {
            total: 1200,
            idle: 900,
        };

        assert_eq!(cpu_usage(previous, current), 50.0);
    }

    #[test]
    fn handles_zero_cpu_delta() {
        let times = CpuTimes {
            total: 1000,
            idle: 800,
        };

        assert_eq!(cpu_usage(times, times), 0.0);
    }
}
