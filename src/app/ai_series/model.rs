use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SeriesEntrant {
    pub(crate) slot_index: u8,
    pub(crate) name: String,
    pub(crate) country: String,
    pub(crate) commander: Option<String>,
    pub(crate) color: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SeriesMap {
    pub(crate) identity: String,
    pub(crate) content_sha256: String,
    pub(crate) map_seed: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SeriesDescriptor {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) round_limit: u16,
    pub(crate) entrants: Vec<SeriesEntrant>,
    pub(crate) maps: Vec<SeriesMap>,
    pub(crate) seeds: Vec<u32>,
}

pub(crate) fn validate_series_id(id: &str) -> Result<()> {
    ensure!(
        !id.is_empty()
            && id.len() <= 80
            && id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte)),
        "Series ID must contain 1–80 letters, digits, underscores, or hyphens"
    );
    Ok(())
}

fn valid_label(value: &str, limit: usize) -> bool {
    !value.trim().is_empty() && value.len() <= limit && !value.chars().any(char::is_control)
}

impl SeriesDescriptor {
    pub(crate) fn validate(&self) -> Result<()> {
        validate_series_id(&self.id)?;
        ensure!(
            valid_label(&self.name, 160),
            "Series name must contain 1–160 printable bytes"
        );
        ensure!(
            (1..=100).contains(&self.round_limit),
            "Choose between 1 and 100 matches"
        );
        ensure!(
            self.seeds.len() == usize::from(self.round_limit),
            "Series needs one seed per match"
        );
        ensure!(
            (2..=29).contains(&self.entrants.len()),
            "A series needs 2–29 AI entrants"
        );
        let mut slots = BTreeSet::new();
        for entrant in &self.entrants {
            ensure!(
                (1..=29).contains(&entrant.slot_index),
                "AI slot is outside the Skirmish roster"
            );
            ensure!(
                slots.insert(entrant.slot_index),
                "Series contains a duplicate AI slot"
            );
            ensure!(
                valid_label(&entrant.name, 160),
                "AI name is empty or too long"
            );
            ensure!(
                valid_label(&entrant.country, 160),
                "AI country is empty or too long"
            );
            ensure!(
                valid_label(&entrant.color, 160),
                "AI color is empty or too long"
            );
            ensure!(
                entrant
                    .commander
                    .as_ref()
                    .is_none_or(|name| valid_label(name, 160)),
                "AI commander is empty or too long"
            );
        }
        ensure!(
            (1..=100).contains(&self.maps.len()),
            "Choose between 1 and 100 maps"
        );
        for map in &self.maps {
            ensure!(
                valid_label(&map.identity, 1024),
                "Map identity is empty or too long"
            );
            ensure!(
                map.content_sha256.len() == 64
                    && map
                        .content_sha256
                        .bytes()
                        .all(|byte| byte.is_ascii_hexdigit()),
                "Map fingerprint must be a SHA-256 digest"
            );
        }
        Ok(())
    }

    pub(crate) fn seed(&self, round: u16) -> Option<u32> {
        round
            .checked_sub(1)
            .and_then(|index| self.seeds.get(usize::from(index)))
            .copied()
    }

    pub(crate) fn map_index(&self, round: u16) -> Option<usize> {
        (round > 0 && round <= self.round_limit && !self.maps.is_empty())
            .then(|| usize::from(round - 1) % self.maps.len())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum RoundOutcome {
    Winner { slot_index: u8 },
    Draw,
    Skipped,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RoundRecord {
    pub(crate) series_id: String,
    pub(crate) round: u16,
    pub(crate) seed: u32,
    pub(crate) map_index: usize,
    pub(crate) outcome: RoundOutcome,
}

impl RoundRecord {
    pub(crate) fn validate(&self, series: &SeriesDescriptor) -> Result<()> {
        ensure!(
            self.series_id == series.id,
            "Match belongs to another series"
        );
        ensure!(
            series.seed(self.round) == Some(self.seed),
            "Match seed differs from its schedule"
        );
        ensure!(
            series.map_index(self.round) == Some(self.map_index),
            "Match map differs from its schedule"
        );
        if let RoundOutcome::Winner { slot_index } = self.outcome {
            ensure!(
                series
                    .entrants
                    .iter()
                    .any(|entrant| entrant.slot_index == slot_index),
                "Match winner is not a series entrant"
            );
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SeriesStanding {
    pub(crate) slot_index: u8,
    pub(crate) wins: u16,
    pub(crate) draws: u16,
    pub(crate) completed: u16,
}

pub(crate) struct SeriesLedger {
    descriptor: SeriesDescriptor,
    records: BTreeMap<u16, RoundRecord>,
}

impl SeriesLedger {
    pub(crate) fn new(descriptor: SeriesDescriptor) -> Result<Self> {
        descriptor.validate()?;
        Ok(Self {
            descriptor,
            records: BTreeMap::new(),
        })
    }

    pub(crate) fn descriptor(&self) -> &SeriesDescriptor {
        &self.descriptor
    }

    pub(crate) fn records(&self) -> &BTreeMap<u16, RoundRecord> {
        &self.records
    }

    pub(crate) fn record(&mut self, record: RoundRecord) -> Result<bool> {
        record.validate(&self.descriptor)?;
        if let Some(previous) = self.records.get(&record.round) {
            ensure!(previous == &record, "Match already has a different result");
            return Ok(false);
        }
        self.records.insert(record.round, record);
        Ok(true)
    }

    pub(crate) fn next_round(&self) -> Option<u16> {
        (1..=self.descriptor.round_limit).find(|round| !self.records.contains_key(round))
    }

    pub(crate) fn is_finished(&self) -> bool {
        self.next_round().is_none()
    }

    pub(crate) fn standings(&self) -> Vec<SeriesStanding> {
        let mut standings: Vec<_> = self
            .descriptor
            .entrants
            .iter()
            .map(|entrant| SeriesStanding {
                slot_index: entrant.slot_index,
                wins: 0,
                draws: 0,
                completed: 0,
            })
            .collect();
        for record in self.records.values() {
            if record.outcome == RoundOutcome::Skipped {
                continue;
            }
            for standing in &mut standings {
                standing.completed += 1;
                match record.outcome {
                    RoundOutcome::Winner { slot_index } if slot_index == standing.slot_index => {
                        standing.wins += 1
                    }
                    RoundOutcome::Draw => standing.draws += 1,
                    _ => {}
                }
            }
        }
        standings.sort_by_key(|standing| {
            (
                std::cmp::Reverse(standing.wins),
                std::cmp::Reverse(standing.draws),
                standing.slot_index,
            )
        });
        standings
    }
}

pub(crate) struct SeriesCountdown {
    round: u16,
    remaining_ms: u64,
    started_ms: Option<u64>,
    cancelled: bool,
    action_taken: bool,
}

impl SeriesCountdown {
    pub(crate) fn new(round: u16, now_ms: u64) -> Self {
        Self {
            round,
            remaining_ms: 10_000,
            started_ms: Some(now_ms),
            cancelled: false,
            action_taken: false,
        }
    }

    fn remaining_ms(&self, now_ms: u64) -> u64 {
        self.remaining_ms.saturating_sub(
            self.started_ms
                .map_or(0, |start| now_ms.saturating_sub(start)),
        )
    }

    pub(crate) fn remaining_seconds(&self, now_ms: u64) -> u64 {
        self.remaining_ms(now_ms).div_ceil(1_000)
    }

    #[cfg(test)]
    pub(crate) fn paused(&self) -> bool {
        self.started_ms.is_none() && !self.cancelled && !self.action_taken
    }

    pub(crate) fn pause(&mut self, now_ms: u64) {
        self.remaining_ms = self.remaining_ms(now_ms);
        self.started_ms = None;
    }

    pub(crate) fn resume(&mut self, now_ms: u64) {
        if !self.cancelled && !self.action_taken && self.started_ms.is_none() {
            self.started_ms = Some(now_ms);
        }
    }

    pub(crate) fn cancel(&mut self) {
        self.cancelled = true;
        self.started_ms = None;
    }

    pub(crate) fn stop_after_current(&mut self) {
        self.cancel();
    }

    pub(crate) fn take_ready(&mut self, now_ms: u64) -> Option<u16> {
        if self.cancelled
            || self.action_taken
            || self.started_ms.is_none()
            || self.remaining_ms(now_ms) > 0
        {
            return None;
        }
        self.action_taken = true;
        Some(self.round)
    }

    pub(crate) fn take_now(&mut self) -> Option<u16> {
        if self.cancelled || self.action_taken {
            return None;
        }
        self.action_taken = true;
        Some(self.round)
    }
}

#[cfg(test)]
pub(super) fn test_descriptor() -> SeriesDescriptor {
    SeriesDescriptor {
        id: "test-series".into(),
        name: "AI Cup".into(),
        round_limit: 3,
        entrants: (1..=2)
            .map(|slot_index| SeriesEntrant {
                slot_index,
                name: format!("Computer{slot_index}"),
                country: "Americans".into(),
                commander: None,
                color: slot_index.to_string(),
            })
            .collect(),
        maps: vec![SeriesMap {
            identity: "arena.map".into(),
            content_sha256: "a".repeat(64),
            map_seed: None,
        }],
        seeds: vec![11, 22, 33],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pause_resume_and_cancel_at_the_deadline_admit_at_most_one_launch() {
        let mut countdown = SeriesCountdown::new(2, 100);
        countdown.pause(4_100);
        assert!(countdown.paused());
        assert_eq!(countdown.remaining_seconds(99_000), 6);
        assert_eq!(countdown.take_ready(99_000), None);
        countdown.resume(100_000);
        assert_eq!(countdown.take_ready(105_999), None);
        assert_eq!(countdown.take_ready(106_000), Some(2));
        assert_eq!(countdown.take_now(), None);
        assert_eq!(countdown.take_ready(107_000), None);
        let mut cancelled = SeriesCountdown::new(3, 0);
        cancelled.cancel();
        cancelled.resume(10_000);
        assert_eq!(cancelled.take_ready(10_000), None);
        assert_eq!(cancelled.take_now(), None);
    }

    #[test]
    fn standings_count_each_finished_round_once_and_leave_skips_unscored() {
        let mut ledger = SeriesLedger::new(test_descriptor()).unwrap();
        let win = RoundRecord {
            series_id: "test-series".into(),
            round: 1,
            seed: 11,
            map_index: 0,
            outcome: RoundOutcome::Winner { slot_index: 2 },
        };
        assert!(ledger.record(win.clone()).unwrap());
        assert!(!ledger.record(win).unwrap());
        ledger
            .record(RoundRecord {
                series_id: "test-series".into(),
                round: 2,
                seed: 22,
                map_index: 0,
                outcome: RoundOutcome::Draw,
            })
            .unwrap();
        ledger
            .record(RoundRecord {
                series_id: "test-series".into(),
                round: 3,
                seed: 33,
                map_index: 0,
                outcome: RoundOutcome::Skipped,
            })
            .unwrap();
        assert!(ledger.is_finished());
        assert_eq!(
            ledger.standings(),
            vec![
                SeriesStanding {
                    slot_index: 2,
                    wins: 1,
                    draws: 1,
                    completed: 2
                },
                SeriesStanding {
                    slot_index: 1,
                    wins: 0,
                    draws: 1,
                    completed: 2
                }
            ]
        );
        let mut invalid = test_descriptor();
        invalid.round_limit = 101;
        assert!(SeriesLedger::new(invalid).is_err());
    }
}
