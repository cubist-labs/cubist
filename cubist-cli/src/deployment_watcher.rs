use std::path::Path;
use std::time::Duration;
use std::{ffi::OsStr, path::PathBuf};

use notify::{
    event::ModifyKind, event::RenameMode, Config, Event, EventKind, PollWatcher, Watcher,
};

use futures::{channel::mpsc::Sender, future::JoinAll, SinkExt};

use cubist_sdk::core::DeploymentManifest;

use crate::stylist;
use eyre::Result;
use tokio::fs;
use tracing::{debug, trace, warn};

pub type DeploymentManifestWithPath = (PathBuf, DeploymentManifest);

pub struct DeploymentWatcher {
    watcher: PollWatcher,
}

impl DeploymentWatcher {
    /// Factory method.
    ///
    /// # Arguments
    /// * `notify` - channel through which to send newly discovered deployment manifests.
    /// * `poll_interval` - how often to poll the filesystem.
    pub fn new(
        mut notify: Sender<DeploymentManifestWithPath>,
        poll_interval: Duration,
    ) -> Result<Self> {
        let tokio_runtime = tokio::runtime::Handle::current();
        let event_handler = move |ev| {
            tokio_runtime.block_on(async {
                let events = Self::process_watcher_event(ev).await;
                for dm in events {
                    notify.send(dm).await.unwrap_or_else(|e| {
                        let msg = format!("Failed to send deployment manifest: {e}");
                        println!("{}", stylist::warning(&msg));
                    });
                }
            })
        };
        let watcher = PollWatcher::new(
            event_handler,
            Config::default().with_poll_interval(poll_interval),
        )?;

        Ok(DeploymentWatcher { watcher })
    }

    /// Adds `dir` to the list of watched directories and returns.
    ///
    /// # Arguments
    /// * `dir` - directory to add to the watch list
    ///
    /// # Panics
    /// * if `dir` is not a directory.
    pub fn watch(&mut self, dir: &Path) -> Result<()> {
        debug_assert!(dir.is_dir());
        self.watcher
            .watch(dir, notify::RecursiveMode::NonRecursive)?;
        Ok(())
    }

    /// From a given filesystem event infers if any new deployment
    /// manifest files were produced.  If so, they are deserialized
    /// into [`DeploymentManifest`] instances and returned (invalid
    /// manifest files are ignored.
    ///
    /// # Arguments
    /// * `maybe_event` - an event received from the filesystem watcher
    ///
    /// # Returns
    /// Newly created deployment manifests or an empty vector.
    async fn process_watcher_event(
        maybe_event: Result<Event, notify::Error>,
    ) -> Vec<DeploymentManifestWithPath> {
        trace!("Detected filesystem event: {maybe_event:?}");
        match maybe_event {
            Ok(event) => match event.kind {
                // If we use a [PollWatcher], then the [notify::event::CreateKind] is
                // [notify::event::CreateKind::Any] (observed on macOS), whereas with the
                // [notify::RecommendedWatcher], it may be [notify::event::CreateKind::File].
                // Similarly, [EventKind::Access] events seem to be not emitted when creating a new
                // file and using the [notify::PollWatcher]. Note that we assume that the file is
                // created atomically (i.e., we assume that we only get notified once the file has
                // been flushed to disk). We ensure this by writing and then renaming the file in
                // [DeploymentManifest::write_atomic()]. Depending on which kind of watcher is used,
                // this change can manifest itself as either [notify::EventKind::Modify] or
                // [notify::EventKind::Create] event.
                EventKind::Create(_) | EventKind::Modify(ModifyKind::Name(RenameMode::To)) => event
                    .paths
                    .iter()
                    .filter(|p| p.is_file() && p.extension() == Some(OsStr::new("json")))
                    .map(|p| Self::try_load_manifest(p))
                    .collect::<JoinAll<_>>()
                    .await
                    .into_iter()
                    .flatten()
                    .collect(),
                _ => {
                    vec![]
                }
            },
            Err(e) => {
                warn!("Error from file watcher: {e}");
                vec![]
            }
        }
    }

    /// Queries a given directory for all files and attempts to
    /// deserialize each into a `DeploymentManifest`.  Deserialization
    /// errors are logged and ignored.  IO errors are propagated back
    /// to the caller.
    ///
    /// # Arguments
    /// * `manifest_dir` - directory to search for deployment manifest files.
    pub async fn find_existing_deployments(
        manifest_dir: &Path,
    ) -> Result<Vec<DeploymentManifestWithPath>> {
        if !manifest_dir.is_dir() {
            return Ok(vec![]);
        }

        // read all directory entries
        let mut result = Vec::new();
        let mut read_result = fs::read_dir(manifest_dir).await?;
        while let Some(dir_entry) = read_result.next_entry().await? {
            // ignore all unreadable/invalid files
            if let Some(manifest) = Self::try_load_manifest(&dir_entry.path()).await {
                result.push(manifest);
            }
        }

        Ok(result)
    }

    /// Tries to deserialize a given file into a [`DeploymentManifest`].
    /// In case of an error, a warning is logged and `None` is returned.
    ///
    /// # Arguments
    /// * `path` - file to attempt to deserialize into [`DeploymentManifest`]
    pub async fn try_load_manifest(path: &Path) -> Option<DeploymentManifestWithPath> {
        // ignore all non-files
        if !path.is_file() {
            trace!("Cannot load manifest from a non-file {}", path.display());
            return None;
        }

        // warn and continue if the file cannot be read
        trace!("Trying to load manifest from {}", path.display());
        let manifest_contents = fs::read_to_string(&path).await;
        if let Err(e) = manifest_contents {
            warn!(
                "Cannot read deployment manifest file '{}': {e}",
                path.display()
            );
            return None;
        }

        // warn and continue if the file cannot be deserialized
        let manifest_contents = manifest_contents.unwrap();
        let manifest = serde_json::from_str::<DeploymentManifest>(&manifest_contents);
        if let Err(e) = manifest {
            warn!("Invalid deployment manifest file '{}': {e}", path.display());
            return None;
        }

        debug!("Loaded deployment manifest from {}", path.display());
        Some((path.to_path_buf(), manifest.unwrap()))
    }
}

#[cfg(test)]
mod tests {
    use cubist_config::{paths::ContractFQN, Target};
    use cubist_sdk::core::DeploymentInfo;
    use ethers_core::abi::Address;
    use futures::{channel::mpsc, StreamExt};
    use std::collections::HashSet as Set;
    use std::path::PathBuf;
    use tempfile::tempdir;
    use tokio::time::timeout;

    use super::*;

    fn random_address() -> Vec<u8> {
        Address::random().to_fixed_bytes().into()
    }

    #[tokio::test]
    async fn load_manifest_non_existent() {
        assert_eq!(
            None,
            DeploymentWatcher::try_load_manifest(&PathBuf::from("foo").join("bar")).await
        );
    }

    #[tokio::test]
    async fn load_manifest_directory() -> Result<()> {
        let tmp = tempdir()?;
        assert_eq!(None, DeploymentWatcher::try_load_manifest(tmp.path()).await);
        Ok(())
    }

    #[tokio::test]
    async fn load_manifest_bogus_contents() -> Result<()> {
        let tmp = tempdir()?;
        let man_file = tmp.path().join("man.json");
        fs::write(&man_file, "foo").await?;
        assert_eq!(None, DeploymentWatcher::try_load_manifest(&man_file).await);
        Ok(())
    }

    #[tokio::test]
    async fn load_manifest_empty() -> Result<()> {
        let tmp = tempdir()?;
        let man_file = tmp.path().join("man.json");
        let dm = DeploymentManifest {
            contract: ContractFQN::new(PathBuf::from("foo.sol"), "foo".to_string()),
            deployment: DeploymentInfo {
                target: Target::Ethereum,
                address: random_address(),
            },
            shims: vec![],
        };
        dm.write_atomic(&man_file)?;
        assert_eq!(
            Some((man_file.clone(), dm)),
            DeploymentWatcher::try_load_manifest(&man_file).await
        );
        Ok(())
    }

    #[tokio::test]
    async fn try_load_non_empty() -> Result<()> {
        let tmp = tempdir()?;
        let man_file = tmp.path().join("man.json");
        let dm = DeploymentManifest {
            contract: ContractFQN::new(PathBuf::from("foo.sol"), "foo".to_string()),
            deployment: DeploymentInfo {
                target: Target::Ethereum,
                address: random_address(),
            },
            shims: vec![DeploymentInfo {
                target: Target::Polygon,
                address: random_address(),
            }],
        };
        dm.write_atomic(&man_file)?;
        assert_eq!(
            Some((man_file.clone(), dm)),
            DeploymentWatcher::try_load_manifest(&man_file).await
        );
        Ok(())
    }

    #[tokio::test]
    async fn find_existing_deployments_missing_dir_fails() -> Result<()> {
        let tmp = tempdir()?;
        let result = DeploymentWatcher::find_existing_deployments(&tmp.path().join("foo")).await?;
        assert_eq!(0, result.len());
        Ok(())
    }

    #[tokio::test]
    async fn find_existing_deployments_empty() -> Result<()> {
        let tmp = tempdir()?;
        assert_eq!(
            0,
            DeploymentWatcher::find_existing_deployments(tmp.path())
                .await?
                .len()
        );

        let man_file = tmp.path().join("man.json");
        let dm = DeploymentManifest {
            contract: ContractFQN::new(PathBuf::from("foo.sol"), "foo".to_string()),
            deployment: DeploymentInfo {
                target: Target::Ethereum,
                address: random_address(),
            },
            shims: vec![DeploymentInfo {
                target: Target::Polygon,
                address: random_address(),
            }],
        };
        // write a good deployment
        fs::write(&man_file, serde_json::to_string(&dm)?).await?;
        assert_eq!(
            vec![(man_file.clone(), dm.clone())],
            DeploymentWatcher::find_existing_deployments(tmp.path()).await?
        );
        // write a bogus file too
        fs::write(&man_file.with_extension("blahblah"), "123").await?;
        assert_eq!(
            vec![(man_file, dm)],
            DeploymentWatcher::find_existing_deployments(tmp.path()).await?
        );
        Ok(())
    }

    #[tokio::test]
    async fn watch() -> Result<()> {
        let watched_dir = tempdir()?;
        let watched_path = watched_dir.path();
        let (tx, mut rx) = mpsc::channel(10);
        let mut watcher = DeploymentWatcher::new(tx, Duration::from_millis(100))?;
        watcher.watch(watched_path)?;

        let dm = DeploymentManifest {
            contract: ContractFQN::new(PathBuf::from("foo.sol"), "foo".to_string()),
            deployment: DeploymentInfo {
                target: Target::Ethereum,
                address: random_address(),
            },
            shims: vec![
                DeploymentInfo {
                    target: Target::Polygon,
                    address: random_address(),
                },
                DeploymentInfo {
                    target: Target::Avalanche,
                    address: random_address(),
                },
            ],
        };

        // write some manifests and some bogus files and assert that only manifests are reported
        let dm1_path = watched_path.join("file1.json");
        let dm2_path = watched_path.join("file2.json");
        dm.write_atomic(&dm1_path)?;
        dm.write_atomic(&dm2_path)?;
        fs::write(&watched_path.join("file3.json"), "bogus contents").await?;

        let mut events = Set::new();
        for _ in 0..2 {
            events.insert(timeout(Duration::from_secs(2), rx.next()).await?);
        }

        assert_eq!(
            Set::from([Some((dm1_path, dm.clone())), Some((dm2_path, dm))]),
            events
        );

        // delete-recreate watched dir to ensure that the watcher continues to work even when that happens
        fs::remove_dir_all(watched_path).await?;
        fs::create_dir_all(watched_path).await?;

        // assert that empty manifests are reported as well
        let dm = DeploymentManifest {
            contract: ContractFQN::new(PathBuf::from("foo.sol"), "foo".to_string()),
            deployment: DeploymentInfo {
                target: Target::Ethereum,
                address: random_address(),
            },
            shims: vec![],
        };
        let dm3_path = watched_path.join("empty.json");
        dm.write_atomic(&dm3_path)?;
        assert_eq!(
            Some((dm3_path, dm)),
            timeout(Duration::from_secs(2), rx.next()).await?
        );

        Ok(())
    }
}
