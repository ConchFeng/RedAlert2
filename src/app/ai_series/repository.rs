use std::io;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use serde::Serialize;
use serde::de::DeserializeOwned;

use super::model::{RoundRecord, SeriesDescriptor, SeriesLedger, validate_series_id};
use crate::util::file_system;

const MAX_JSON_BYTES: usize = 8 * 1024 * 1024;

pub(crate) struct RoundRecoveryIssue {
    pub(crate) round: u16,
    pub(crate) message: String,
}

pub(crate) struct RecoveredSeries {
    pub(crate) ledger: SeriesLedger,
    pub(crate) issues: Vec<RoundRecoveryIssue>,
}

pub(crate) struct SeriesRepository {
    directory: PathBuf,
    id: String,
}

impl SeriesRepository {
    pub(crate) fn at(settings_root: impl AsRef<Path>, id: &str) -> Result<Self> {
        validate_series_id(id)?;
        Ok(Self {
            directory: settings_root.as_ref().join("ai-series").join(id),
            id: id.to_owned(),
        })
    }

    #[cfg(test)]
    pub(crate) fn directory(&self) -> &Path {
        &self.directory
    }

    pub(crate) fn create(&self, descriptor: &SeriesDescriptor) -> Result<bool> {
        descriptor.validate()?;
        ensure!(
            descriptor.id == self.id,
            "Series descriptor belongs to another directory"
        );
        file_system::create_dir_all(&self.directory).context("Create AI series directory")?;
        publish_json(&self.directory.join("series.json"), descriptor)
    }

    pub(crate) fn save_launch<T: Serialize>(&self, launch: &T) -> Result<bool> {
        let descriptor: SeriesDescriptor = read_json(&self.directory.join("series.json"))?;
        ensure!(
            descriptor.id == self.id,
            "Saved series belongs to another directory"
        );
        descriptor.validate()?;
        let value = serde_json::to_value(launch)?;
        publish_json(&self.directory.join("launch.json"), &value)
    }

    pub(crate) fn load_launch<T: DeserializeOwned>(&self) -> Result<T> {
        read_json(&self.directory.join("launch.json"))
    }

    pub(crate) fn save_round_launch<T: Serialize>(
        &self,
        descriptor: &SeriesDescriptor,
        round: u16,
        launch: &T,
    ) -> Result<bool> {
        self.validate_round(descriptor, round)?;
        publish_json(
            &self.round_launch_path(round),
            &serde_json::to_value(launch)?,
        )
    }

    pub(crate) fn load_round_launch<T: DeserializeOwned>(
        &self,
        descriptor: &SeriesDescriptor,
        round: u16,
    ) -> Result<Option<T>> {
        self.validate_round(descriptor, round)?;
        let path = self.round_launch_path(round);
        match read_bytes(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .with_context(|| format!("Decode {}", path.display())),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error).with_context(|| format!("Read {}", path.display())),
        }
    }

    fn validate_round(&self, descriptor: &SeriesDescriptor, round: u16) -> Result<()> {
        descriptor.validate()?;
        ensure!(
            descriptor.id == self.id,
            "Series descriptor belongs to another directory"
        );
        ensure!(
            descriptor.seed(round).is_some(),
            "Match is outside the series schedule"
        );
        let saved: SeriesDescriptor = read_json(&self.directory.join("series.json"))?;
        ensure!(
            saved == *descriptor,
            "Saved series schedule differs from the active series"
        );
        Ok(())
    }

    fn round_launch_path(&self, round: u16) -> PathBuf {
        self.directory.join(format!("round-{round:04}-launch.json"))
    }

    pub(crate) fn recover(&self) -> Result<RecoveredSeries> {
        let descriptor: SeriesDescriptor = read_json(&self.directory.join("series.json"))?;
        ensure!(
            descriptor.id == self.id,
            "Saved series belongs to another directory"
        );
        let mut ledger = SeriesLedger::new(descriptor)?;
        let mut issues = Vec::new();
        for round in 1..=ledger.descriptor().round_limit {
            let path = self.round_path(round);
            match read_bytes(&path) {
                Ok(bytes) => {
                    let admitted = serde_json::from_slice::<RoundRecord>(&bytes)
                        .context("Match result is incomplete or invalid")
                        .and_then(|record| {
                            ensure!(
                                record.round == round,
                                "Match result belongs to another round"
                            );
                            ledger.record(record).map(|_| ())
                        });
                    if let Err(error) = admitted {
                        issues.push(RoundRecoveryIssue {
                            round,
                            message: format!("{error:#}"),
                        });
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => issues.push(RoundRecoveryIssue {
                    round,
                    message: error.to_string(),
                }),
            }
        }
        Ok(RecoveredSeries { ledger, issues })
    }

    pub(crate) fn record(
        &self,
        descriptor: &SeriesDescriptor,
        record: &RoundRecord,
    ) -> Result<bool> {
        descriptor.validate()?;
        ensure!(
            descriptor.id == self.id,
            "Series descriptor belongs to another directory"
        );
        record.validate(descriptor)?;
        let saved: SeriesDescriptor = read_json(&self.directory.join("series.json"))?;
        ensure!(
            saved == *descriptor,
            "Saved series schedule differs from the active series"
        );
        publish_json(&self.round_path(record.round), record)
    }

    fn round_path(&self, round: u16) -> PathBuf {
        self.directory.join(format!("round-{round:04}.json"))
    }

    pub(crate) fn list(settings_root: impl AsRef<Path>) -> Result<Vec<SeriesDescriptor>> {
        let root = settings_root.as_ref().join("ai-series");
        let entries = match file_system::read_dir(&root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error).context("Read AI series directory"),
        };
        let mut descriptors = Vec::new();
        for entry in entries {
            let entry = entry.context("Read AI series entry")?;
            if !entry.metadata()?.is_dir() {
                continue;
            }
            let Some(id) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if validate_series_id(&id).is_err() {
                continue;
            }
            let descriptor = read_json::<SeriesDescriptor>(&entry.path().join("series.json"))
                .and_then(|descriptor| {
                    descriptor.validate()?;
                    ensure!(
                        descriptor.id == id,
                        "Saved series belongs to another directory"
                    );
                    Ok(descriptor)
                });
            match descriptor {
                Ok(descriptor) => descriptors.push(descriptor),
                Err(error) => log::warn!("Could not read saved AI series {id}: {error:#}"),
            }
        }
        descriptors.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(descriptors)
    }
}

fn read_bytes(path: &Path) -> io::Result<Vec<u8>> {
    if file_system::metadata(path)?.len() > MAX_JSON_BYTES as u64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "AI series file exceeds 8 MiB",
        ));
    }
    let bytes = file_system::read(path)?;
    if bytes.len() > MAX_JSON_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "AI series file exceeds 8 MiB",
        ));
    }
    Ok(bytes)
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let bytes = read_bytes(path).with_context(|| format!("Read {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("Decode {}", path.display()))
}

fn publish_json<T: Serialize + DeserializeOwned + PartialEq>(
    path: &Path,
    value: &T,
) -> Result<bool> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    ensure!(
        bytes.len() <= MAX_JSON_BYTES,
        "AI series file exceeds 8 MiB"
    );
    match publish_new(path, &bytes) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            let previous: T = read_json(path)?;
            ensure!(
                &previous == value,
                "{} already contains a different result",
                path.display()
            );
            Ok(false)
        }
        Err(error) => Err(error).with_context(|| format!("Save {}", path.display())),
    }
}

#[cfg(all(feature = "wasm", target_arch = "wasm32", target_os = "unknown"))]
fn publish_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    file_system::write_new(path, bytes)
}

#[cfg(not(all(feature = "wasm", target_arch = "wasm32", target_os = "unknown")))]
fn publish_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_STAGE: AtomicU64 = AtomicU64::new(0);
    let directory = path.parent().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "Series file needs a directory")
    })?;
    let (stage, mut file) = loop {
        let id = NEXT_STAGE.fetch_add(1, Ordering::Relaxed);
        let stage = directory.join(format!(".pending-{}-{id}.json", std::process::id()));
        match file_system::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&stage)
        {
            Ok(file) => break (stage, file),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    };
    let written = file.write_all(bytes).and_then(|()| file.sync_all());
    drop(file);
    let published = written.and_then(|()| file_system::hard_link(&stage, path));
    let _ = file_system::remove_file(&stage);
    published?;
    #[cfg(unix)]
    file_system::File::open(directory)?.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::ai_series::model::{RoundOutcome, test_descriptor};

    #[test]
    fn duplicate_results_recover_once_and_a_partial_round_remains_incomplete() {
        let directory = std::env::temp_dir().join(format!(
            "vera-series-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let descriptor = test_descriptor();
        let repository = SeriesRepository::at(&directory, &descriptor.id).unwrap();
        assert!(repository.create(&descriptor).unwrap());
        assert!(!repository.create(&descriptor).unwrap());
        assert!(repository.save_launch(&vec![11_u32, 22]).unwrap());
        assert!(!repository.save_launch(&vec![11_u32, 22]).unwrap());
        assert!(repository.save_launch(&vec![99_u32]).is_err());
        assert_eq!(repository.load_launch::<Vec<u32>>().unwrap(), vec![11, 22]);
        assert_eq!(
            repository
                .load_round_launch::<Vec<u32>>(&descriptor, 1)
                .unwrap(),
            None
        );
        assert!(
            repository
                .save_round_launch(&descriptor, 1, &vec![11_u32])
                .unwrap()
        );
        assert!(
            !repository
                .save_round_launch(&descriptor, 1, &vec![11_u32])
                .unwrap()
        );
        assert!(
            repository
                .save_round_launch(&descriptor, 1, &vec![99_u32])
                .is_err()
        );
        assert_eq!(
            repository
                .load_round_launch::<Vec<u32>>(&descriptor, 1)
                .unwrap(),
            Some(vec![11])
        );
        assert!(
            repository
                .load_round_launch::<Vec<u32>>(&descriptor, 0)
                .is_err()
        );
        let record = RoundRecord {
            series_id: descriptor.id.clone(),
            round: 1,
            seed: 11,
            map_index: 0,
            outcome: RoundOutcome::Winner { slot_index: 2 },
        };
        assert!(repository.record(&descriptor, &record).unwrap());
        assert!(!repository.record(&descriptor, &record).unwrap());
        let mut conflicting = record.clone();
        conflicting.outcome = RoundOutcome::Draw;
        assert!(repository.record(&descriptor, &conflicting).is_err());
        file_system::write(repository.round_path(2), b"{\"round\":2,").unwrap();
        let recovered = repository.recover().unwrap();
        assert_eq!(recovered.ledger.records().len(), 1);
        assert_eq!(recovered.ledger.next_round(), Some(2));
        assert_eq!(recovered.ledger.standings()[0].wins, 1);
        assert_eq!(recovered.issues.len(), 1);
        assert_eq!(recovered.issues[0].round, 2);
        assert!(!recovered.issues[0].message.is_empty());
        assert_eq!(
            SeriesRepository::list(&directory).unwrap(),
            vec![descriptor]
        );
        assert!(SeriesRepository::at(&directory, "../other").is_err());
        file_system::remove_dir_all(directory).unwrap();
    }
}
