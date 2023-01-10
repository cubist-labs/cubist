use std::{
    borrow::Cow,
    collections::HashMap,
    env::consts::{ARCH, OS},
    path::PathBuf,
};

use cubist_config::util::OrBug;
use tokio::{
    fs::{self, File},
    io::AsyncWriteExt,
};

use crate::{
    error::{DownloadError, Error, Result},
    progress::{DownloadPb, ExtractPb},
};
use flate2::read::GzDecoder;
use once_cell::sync::Lazy;
use reqwest::get;
use serde::{
    de::{SeqAccess, Visitor},
    ser::SerializeSeq,
    Deserialize, Deserializer, Serialize, Serializer,
};
use tar::Archive;
use url::Url;

pub type HashBytes = [u8; 32];

pub type Manifest = HashMap<String, HashMap<String, HashMap<String, ResourceInfo>>>;
static MANIFEST: Lazy<Manifest> = Lazy::new(|| {
    toml::from_str::<Manifest>(include_str!("../data/resources.toml"))
        .or_bug("Malformed resource manifest")
});

pub static DEFAULT_CACHE: Lazy<PathBuf> = Lazy::new(|| {
    dirs::cache_dir()
        .or_bug("unable to find cachedir")
        .join("cubist_localchains")
});

#[derive(Deserialize, Serialize)]
pub struct ResourceInfo {
    /// Url from where to download this resource.
    pub url: Url,

    /// Relative paths of the binaries (inside the downloaded archive) to extract.
    /// Must be non-empty.  The first listed binary must correspond to the main executable.
    pub binaries: Vec<PathBuf>,

    /// Expected Blake3 hashes of the files in `binaries. The length must match the length of `binaries`.
    #[serde(
        default,
        serialize_with = "as_base64",
        deserialize_with = "from_base64"
    )]
    pub hashes: Vec<HashBytes>,
}

impl ResourceInfo {
    pub fn zip_binaries(&self) -> Result<Vec<(PathBuf, blake3::Hash)>> {
        Ok(self
            .binaries
            .iter()
            .zip(self.hashes.iter().map(|h| (*h).into()))
            .map(|t| (t.0.clone(), t.1))
            .collect())
    }

    pub fn validate(&self) -> Result<()> {
        if self.binaries.is_empty() {
            return Err(Error::MissingBinaries(self.url.to_string()));
        }
        if self.binaries.len() != self.hashes.len() {
            return Err(Error::MismatchedBinariesAndHashes {
                num_binaries: self.binaries.len(),
                num_hashes: self.hashes.len(),
            });
        }
        Ok(())
    }
}

pub fn resource_for_current_machine(name: &str) -> crate::Result<Downloadable> {
    // TODO: We should consider using a compile-time generated enum to prevent this type of error.
    // However, it should show up pretty quickly during testing.
    let product = MANIFEST
        .get(name)
        .or_bug(&format!("Unknown resource {name}"));

    let info = match product.get(OS).and_then(|m| m.get(ARCH)) {
        Some(info) => info,
        None => {
            let supported_combos = product.iter().flat_map(|(platform, archs)| {
                archs
                    .keys()
                    .map(|arch| format!("{}-{}", platform, arch))
                    .collect::<Vec<_>>()
                    .into_iter()
            });

            let supported_string = supported_combos.collect::<Vec<_>>().join(", ");
            return Err(Error::UnsupportedPlatformError {
                name: name.to_string(),
                os: OS.to_string(),
                arch: ARCH.to_string(),
                supported: supported_string,
            });
        }
    };

    info.validate()?;
    Ok(Downloadable {
        url: info.url.clone(),
        destination_dir: DEFAULT_CACHE.join(format!("{name}-{OS}-{ARCH}")),
        binaries: info.zip_binaries()?,
    })
}

#[derive(Debug)]
pub struct Downloadable {
    /// URL from which to download the archive.
    pub url: Url,
    /// Directory to which to extract the downloaded archive.
    pub destination_dir: PathBuf,
    /// Relative paths and expected hashes of all binaries to extract from the downloaded archive.
    pub binaries: Vec<(PathBuf, blake3::Hash)>,
}

impl Downloadable {
    /// Main executable.
    pub fn binary(&self) -> &PathBuf {
        &self.binaries[0].0
    }

    /// Name of the main executable.
    pub fn name(&self) -> Cow<str> {
        self.binary()
            .file_name()
            .or_bug("download should always be a file")
            .to_string_lossy()
    }

    /// Destination of the main executable.
    pub fn destination(&self) -> PathBuf {
        self.destination_dir.join(self.binary())
    }

    /// Check if all specified binaries exist and have correct hashes.
    pub async fn exists(&self) -> Result<()> {
        for (path, expected_hash) in &self.binaries {
            let destination = self.destination_dir.join(path);
            let res_hash: blake3::Hash = fs::read(&destination)
                .await
                .map(|bytes| blake3::hash(&bytes))
                .map_err(|_| DownloadError::MissingDownloadedFile(destination.clone()))?;
            if res_hash != *expected_hash {
                return Err(Error::DownloadError(DownloadError::IncorrectHash {
                    file: destination,
                    expected: *expected_hash,
                    actual: res_hash,
                }));
            }
        }

        Ok(())
    }

    /// Download specified binaries
    pub async fn download(&self, pb: Option<&DownloadPb>) -> Result<Vec<u8>> {
        use crate::error::DownloadError::RequestError;

        let mut resp = get(self.url.as_str())
            .await
            .map_err(|e| RequestError(self.url.clone(), e))?;
        if let Some(pb) = pb {
            pb.started(resp.content_length());
        }
        let mut data = vec![];
        while let Some(chunk) = resp
            .chunk()
            .await
            .map_err(|e| RequestError(self.url.clone(), e))?
        {
            data.extend(chunk);
            if let Some(pb) = pb {
                pb.update(data.len() as u64);
            }
        }
        tracing::trace!("Successfully downloaded: {}", self.url);
        Ok(data)
    }

    // Extract downloaded binaries into the destination folder.
    pub async fn extract(&self, data: &[u8], pb: Option<&ExtractPb>) -> Result<()> {
        use crate::error::DownloadError::MalformedDownload;
        use crate::error::DownloadError::SaveError;
        use Error::DownloadError;

        if let Some(pb) = pb {
            pb.started(Some(self.binaries.len() as u64));
        }

        let num_found: usize = async {
            if self.url.as_str().ends_with(".tar.gz") {
                self.extract_from_tar(data, pb).await
            } else {
                let dest = self.destination();
                if let Some(pb) = pb {
                    pb.extracting(&dest);
                }
                let mut file = File::create(&dest).await?;
                file.write_all(data).await?;
                if let Some(pb) = pb {
                    pb.extracted(&dest);
                }
                Ok(1)
            }
        }
        .await
        .map_err(|e| DownloadError(SaveError(self.destination(), e)))?;

        match num_found == self.binaries.len() {
            true => Ok(()),
            false => Err(DownloadError(MalformedDownload(format!(
                "Found only {num_found} binaries out of [{}] in downloaded archive",
                self.binaries
                    .iter()
                    .map(|t| t.0.to_string_lossy())
                    .collect::<Vec<Cow<str>>>()
                    .join(", ")
            )))),
        }
    }

    async fn extract_from_tar(
        &self,
        data: &[u8],
        pb: Option<&ExtractPb>,
    ) -> Result<usize, std::io::Error> {
        tracing::trace!("Attempting to untar-zip file");

        let mut archive = Archive::new(GzDecoder::new(data));
        let mut num_found = 0;
        if self.destination_dir.is_file() {
            fs::remove_file(&self.destination_dir).await?;
        }

        for entry in archive.entries()? {
            let mut entry = entry?;
            let entry_path = entry.path()?;
            let is_found = self.binaries.iter().any(|e| e.0 == entry_path);
            if is_found {
                num_found += 1;
                let destination = self.destination_dir.join(&entry_path);
                if let Some(pb) = pb {
                    pb.extracting(&destination);
                }
                let parent_dir = destination.parent().unwrap();
                fs::create_dir_all(parent_dir).await?;
                entry.unpack(&destination)?;
                if let Some(pb) = pb {
                    pb.extracted(&destination);
                }
            }

            // short-circuit if we found all expected binaries
            if num_found == self.binaries.len() {
                break;
            }
        }

        tracing::trace!("Successfully untar-zip'd file");
        Ok(num_found)
    }
}

fn as_base64<T, S>(key: &Vec<T>, serializer: S) -> Result<S::Ok, S::Error>
where
    T: AsRef<[u8]>,
    S: Serializer,
{
    let mut seq = serializer.serialize_seq(Some(key.len()))?;
    for element in key {
        seq.serialize_element(&base64::encode(element.as_ref()))?;
    }
    seq.end()
}

struct Blake3Deserializer;

impl<'a> Visitor<'a> for Blake3Deserializer {
    type Value = Vec<HashBytes>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("base64 encoding of a blake3 hash")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'a>,
    {
        use serde::de::Error;
        let mut result = Vec::new();
        while let Some(string) = seq.next_element::<String>()? {
            let elem = base64::decode(&string)
                .map_err(|err| Error::custom(err.to_string()))
                .and_then(|bytes| {
                    HashBytes::try_from(bytes).map_err(|_| Error::custom("Incorrect byte length"))
                })?;
            result.push(elem);
        }

        Ok(result)
    }
}

fn from_base64<'a, D>(deserializer: D) -> Result<Vec<[u8; 32]>, D::Error>
where
    D: Deserializer<'a>,
{
    deserializer.deserialize_seq(Blake3Deserializer)
}

#[cfg(test)]
mod tests {
    use super::ResourceInfo;
    use crate::error::Error;
    use core::panic;
    use std::path::PathBuf;

    #[test]
    fn test_validate_mismatched_binaries_and_hashes() {
        let ri = ResourceInfo {
            url: url::Url::parse("http://localhost/foo.tar.gz").unwrap(),
            binaries: vec![PathBuf::from("foo")],
            hashes: vec![],
        };
        match ri.validate() {
            Ok(_) => panic!("Expected error"),
            Err(Error::MismatchedBinariesAndHashes {
                num_binaries,
                num_hashes,
            }) => {
                assert_eq!(1, num_binaries);
                assert_eq!(0, num_hashes);
            }
            e => panic!("Unexpected error: {:?}", e),
        }
    }

    #[test]
    fn test_validate_missing_binaries() {
        let url = url::Url::parse("http://localhost/foo.tar.gz").unwrap();
        let ri = ResourceInfo {
            url: url.clone(),
            binaries: vec![],
            hashes: vec![],
        };
        match ri.validate() {
            Ok(_) => panic!("Expected error"),
            Err(Error::MissingBinaries(u)) => assert_eq!(u, url.to_string()),
            e => panic!("Unexpected error: {:?}", e),
        }
    }

    #[test]
    fn test_zip_binaries_ok() {
        let hash: [u8; 32] = Default::default();
        let bin = PathBuf::from("foo");
        let ri = ResourceInfo {
            url: url::Url::parse("http://localhost/foo.tar.gz").unwrap(),
            binaries: vec![bin.clone()],
            hashes: vec![hash],
        };
        match ri.zip_binaries() {
            Ok(result) => {
                assert_eq!(1, result.len());
                assert_eq!(bin, result[0].0);
                assert_eq!(hash, *result[0].1.as_bytes());
            }
            e => panic!("Unexpected error: {:?}", e),
        }
    }
}
