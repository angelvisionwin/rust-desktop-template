//! Histórico de niveles para estimar el tiempo restante de batería.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const MAX_AGE_SECS: u64 = 7 * 24 * 3600;
const MAX_SAMPLES_PER_DEVICE: usize = 2000;
const MIN_STORED_GAP_SECS: u64 = 5 * 60;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sample {
    pub ts: u64,
    pub percent: u8,
}

#[derive(Default)]
pub struct History {
    samples: BTreeMap<String, Vec<Sample>>,
}

impl History {
    pub fn load() -> History {
        let mut history = History::default();
        let Ok(text) = std::fs::read_to_string(path()) else {
            return history;
        };
        for line in text.lines() {
            let mut fields = line.split('\t');
            let (Some(key), Some(ts), Some(percent)) =
                (fields.next(), fields.next(), fields.next())
            else {
                continue;
            };
            let (Ok(ts), Ok(percent)) = (ts.parse::<u64>(), percent.parse::<u8>()) else {
                continue;
            };
            if key.is_empty() {
                continue;
            }
            history
                .samples
                .entry(key.to_string())
                .or_default()
                .push(Sample { ts, percent });
        }
        for samples in history.samples.values_mut() {
            samples.sort_by_key(|sample| sample.ts);
        }
        history
    }

    pub fn record(&mut self, key: &str, percent: u8, now: u64) {
        let samples = self.samples.entry(key.to_string()).or_default();
        let fresh = match samples.last() {
            Some(last) => {
                last.percent != percent || now.saturating_sub(last.ts) >= MIN_STORED_GAP_SECS
            }
            None => true,
        };
        if fresh {
            samples.push(Sample { ts: now, percent });
        }
        samples.retain(|sample| now.saturating_sub(sample.ts) <= MAX_AGE_SECS);
        if samples.len() > MAX_SAMPLES_PER_DEVICE {
            let excess = samples.len() - MAX_SAMPLES_PER_DEVICE;
            samples.drain(..excess);
        }
    }

    pub fn save(&self) {
        let mut text = String::with_capacity(self.samples.len() * 40);
        for (key, samples) in &self.samples {
            for sample in samples {
                text.push_str(&format!("{key}\t{}\t{}\n", sample.ts, sample.percent));
            }
        }
        let path = path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(error) = std::fs::write(&path, text) {
            crate::util::log(&format!("histórico: no se pudo guardar: {error}"));
        }
    }

    /// Estima el tiempo hasta agotar la batería a partir de la caída observada.
    pub fn estimate(&self, key: &str, percent: u8, now: u64) -> Option<Duration> {
        let samples = self.samples.get(key)?;
        if percent == 0 {
            return Some(Duration::ZERO);
        }
        let mut best: Option<(u64, u8)> = None;
        for sample in samples.iter().rev() {
            if sample.ts > now {
                continue;
            }
            if sample.percent <= percent {
                continue;
            }
            let age = now - sample.ts;
            if age > 48 * 3600 {
                break;
            }
            best = Some((sample.ts, sample.percent));
        }
        let (ts, start_percent) = best?;
        let span_secs = now.saturating_sub(ts);
        let drop = start_percent.saturating_sub(percent);
        // Se exige al menos una hora de observación y una caída apreciable.
        if span_secs < 3600 || drop < 2 {
            return None;
        }
        let rate_per_sec = drop as f64 / span_secs as f64;
        let remaining_secs = percent as f64 / rate_per_sec;
        if !remaining_secs.is_finite() || remaining_secs <= 0.0 {
            return None;
        }
        let capped = remaining_secs.min(30.0 * 86400.0) as u64;
        Some(Duration::from_secs(capped.max(60)))
    }
}

pub fn path() -> PathBuf {
    crate::config::dir().join(crate::meta::HISTORY_FILE)
}

pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn history_with(samples: &[(u64, u8)]) -> History {
        let mut history = History::default();
        let list = samples
            .iter()
            .map(|&(ts, percent)| Sample { ts, percent })
            .collect();
        history.samples.insert("dev".to_string(), list);
        history
    }

    #[test]
    fn estimates_remaining_time_from_drain_rate() {
        let now = 100_000;
        // 10 % en 10 h => 1 %/h; al 40 % quedan ~40 h.
        let history = history_with(&[(now - 10 * 3600, 50), (now - 5 * 3600, 45)]);
        let remaining = history.estimate("dev", 40, now).unwrap();
        assert!(
            (remaining.as_secs() as i64 - 40 * 3600).abs() < 60,
            "estimación inesperada: {remaining:?}"
        );
    }

    #[test]
    fn ignores_short_or_flat_series() {
        let now = 100_000;
        let short = history_with(&[(now - 600, 90)]);
        assert_eq!(short.estimate("dev", 80, now), None);
        let flat = history_with(&[(now - 10 * 3600, 80)]);
        assert_eq!(flat.estimate("dev", 79, now), None);
        assert_eq!(flat.estimate("desconocido", 50, now), None);
    }

    #[test]
    fn full_battery_has_no_estimate_and_zero_is_immediate() {
        let now = 100_000;
        let history = history_with(&[(now - 20 * 3600, 100), (now - 10 * 3600, 95)]);
        assert_eq!(history.estimate("dev", 0, now), Some(Duration::ZERO));
        assert!(history.estimate("dev", 100, now).is_none());
    }

    #[test]
    fn keeps_only_recent_samples_and_compacts_duplicates() {
        let now = 10 * 24 * 3600;
        let mut history = History::default();
        history.record("dev", 80, now - MAX_AGE_SECS - 10);
        history.record("dev", 50, now);
        history.record("dev", 50, now + 1);
        let samples = &history.samples["dev"];
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].percent, 50);
    }

    #[test]
    fn parses_round_trip_lines() {
        let text = "hidpp:046d:C548:2\t1700000000\t90\nbasura\nble:x\t1700000001\t55\n";
        let mut parsed = History::default();
        for line in text.lines() {
            let mut fields = line.split('\t');
            if let (Some(key), Some(ts), Some(percent)) =
                (fields.next(), fields.next(), fields.next())
                && let (Ok(ts), Ok(percent)) = (ts.parse::<u64>(), percent.parse::<u8>())
            {
                parsed
                    .samples
                    .entry(key.to_string())
                    .or_default()
                    .push(Sample { ts, percent });
            }
        }
        assert_eq!(parsed.samples.len(), 2);
        assert_eq!(parsed.samples["hidpp:046d:C548:2"][0].percent, 90);
    }
}
