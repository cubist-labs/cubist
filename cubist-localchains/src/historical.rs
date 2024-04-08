use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::{
    error::{Error, Result},
    resource::DEFAULT_CACHE,
};

#[derive(Serialize, Deserialize, Clone, Debug, Default, Eq, PartialEq)]
pub struct HistoricalMetadata {
    server_boostrap_times: BTreeMap<String, Duration>,
}

pub static DEFAULT_PATH: Lazy<PathBuf> = Lazy::new(|| DEFAULT_CACHE.join("historical.json"));

impl HistoricalMetadata {
    /// Load historical data from disk if found, otherwise return
    /// [`HistoricalMetadata::default()`].
    pub fn load() -> Self {
        match Self::load_from_file(&DEFAULT_PATH) {
            Ok(result) => result,
            Err(e) => {
                debug!("Failed to load historical metadata file: {e}");
                Default::default()
            }
        }
    }

    /// Load historical data from a specific file on disk.
    pub fn load_from_file(path: &Path) -> Result<Self> {
        let s = fs::read_to_string(path).map_err(|e| {
            Error::FsError("Failed to read historical metadata file", path.into(), e)
        })?;
        serde_json::from_str(&s).map_err(|e| {
            Error::JsonError(
                "Failed to deserialize 'HistoricalMetadata'",
                Some(path.into()),
                e,
            )
        })
    }

    /// Save historical data to disk. We don't care about potentially
    /// concurrent accesses to this file; they should be rare and the
    /// failure mode is to just use ballbark durations.
    pub fn save(&self) -> Result<()> {
        self.save_to_file(&DEFAULT_PATH)
    }

    /// Save historical data to a file on disk.
    ///
    /// # Arguments
    /// * `path` - destination of the file.
    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)
                .map_err(|e| Error::FsError("Failed to create directory", dir.into(), e))?;
        }
        let s = serde_json::to_string_pretty(self)
            .map_err(|e| Error::JsonError("Failed to serialize 'HistoricalMetadata'", None, e))?;
        fs::write(path, s).map_err(|e| {
            Error::FsError(
                "Failed to save serialized 'HistoricalMetadata'",
                path.into(),
                e,
            )
        })
    }

    /// Get the last recorded bootstrap duration for a server identified by `key`.
    pub fn get_server_bootstrap_duration(&self, key: &str) -> Option<Duration> {
        self.server_boostrap_times.get(key).cloned()
    }

    /// Update the bootstrap duration for a server identified by `key`.
    /// This update permanently saved only when [`save`](Self::save) is called.
    pub fn set_server_bootstrap_duration(&mut self, key: &str, dur: &Duration) {
        self.server_boostrap_times
            .insert(key.to_owned(), dur.to_owned());
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::HistoricalMetadata;

    #[test]
    fn load_save() {
        let tmp = tempdir::TempDir::new("load_save").unwrap();
        let path = tmp.path().join("hist.json");
        let key1 = "asdfg";
        let dur1 = Duration::from_micros(1234);
        let key2 = "qwert";
        let dur2 = Duration::from_secs(2345);
        assert!(HistoricalMetadata::load_from_file(&path).is_err());
        let mut hist = HistoricalMetadata::default();
        hist.set_server_bootstrap_duration(key1, &dur1);
        assert_eq!(Some(dur1), hist.get_server_bootstrap_duration(key1));
        hist.set_server_bootstrap_duration(key2, &dur1);
        assert_eq!(Some(dur1), hist.get_server_bootstrap_duration(key2));
        hist.set_server_bootstrap_duration(key2, &dur2);
        assert_eq!(Some(dur2), hist.get_server_bootstrap_duration(key2));
        hist.save_to_file(&path).unwrap();
        let hist2 = HistoricalMetadata::load_from_file(&path).unwrap();
        assert_eq!(hist, hist2);
    }
}
