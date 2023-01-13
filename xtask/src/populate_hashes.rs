use color_eyre::eyre::{Result, WrapErr};
use cubist_localchains::resource::{Downloadable, HashBytes, Manifest};
/// This is a convenience script which downloads all the resources listed in
/// `data/resources.toml` and calculates the hash values for all of them.
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::Path,
};
use tempdir::TempDir;

pub async fn run() -> Result<()> {
    let manifest_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("cubist-localchains/data/resources.toml");
    let manifest_string =
        fs::read_to_string(&manifest_path).wrap_err("Could not load manifest file")?;
    let mut manifest: Manifest = toml::from_str(&manifest_string)?;

    let infos: Vec<_> = manifest
        .values_mut()
        .flat_map(|hm| hm.values_mut())
        .flat_map(|hm| hm.values_mut())
        .collect();

    for i in &infos {
        i.validate()?;
    }

    let tempdir = TempDir::new("hasher").wrap_err("Could not create tempdir")?;
    for i in infos {
        let dl = Downloadable {
            url: i.url.clone(),
            destination_dir: tempdir.path().join("current"),
            binaries: i.zip_binaries()?,
        };
        eprintln!("Downloading {}", i.url.as_str());
        let bytes = dl.download(None).await?;
        dl.extract(&bytes, None).await?;

        let mut hashes: Vec<HashBytes> = Vec::new();
        for path in &i.binaries {
            let destination = dl.destination_dir.join(path);
            eprintln!("Hashing {}", destination.display());
            let mut f = File::open(destination).wrap_err("Could not open downloaded file")?;
            let mut bytes = vec![];
            f.read_to_end(&mut bytes)?;
            hashes.push(blake3::hash(&bytes).into());
        }
        i.hashes = hashes;
    }

    let mut manifest_file = File::options()
        .write(true)
        .open(&manifest_path)
        .wrap_err("Error opening manifest file")?;
    manifest_file
        .write_all(toml::to_string_pretty(&manifest)?.as_bytes())
        .wrap_err("Error writing to manifest file")?;

    println!(
        "Successfully updated manifest file: {}",
        manifest_path.display()
    );

    Ok(())
}
