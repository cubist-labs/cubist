#![doc(html_no_source)]
use std::io::{stdout, Write};

use color_eyre::owo_colors::OwoColorize;
use config::configs;
use cubist_config::{network::NetworkProfile, secret::SecretUrl, util::OrBug};
use cubist_util::proc::{kill, SIGTERM};
use error::{Error, Result};
use futures::future::TryJoinAll;
use progress::ServerPb;
use provider::Server;
use tokio::{select, try_join};

use crate::{
    historical::HistoricalMetadata,
    progress::{DownloadPb, ExtractPb, ServerMpb},
};

pub mod config;
pub mod error;
mod historical;
mod progress;
pub mod provider;
mod proxy;
pub mod resource;
mod tracing;

trait UrlExt {
    fn is_loopback(&self) -> Result<bool>;
}

impl UrlExt for SecretUrl {
    fn is_loopback(&self) -> Result<bool> {
        let url = self.expose_url()?;
        // The port number is irrelevant, since we're just doing a dns lookup.
        match url.socket_addrs(|| None).or_bug("No socket_addrs").get(0) {
            Some(it) => Ok(it.ip().is_loopback()),
            None => Err(Error::CannotResolveHostname(
                url.host_str().unwrap_or_default().to_string(),
            )),
        }
    }
}

pub fn to_uri(url: &url::Url) -> hyper::Uri {
    // Safety: valid URLs are valid URIs so this unwrap is safe
    url.as_str().parse().expect("URL must be URI")
}

pub async fn start(profile: &NetworkProfile) -> Result<Vec<Box<dyn Server>>> {
    // Validate the config
    let providers = configs(profile)
        .map(|c| c.provider().map(|p| (c, p)))
        .collect::<Result<Vec<_>>>()?;

    // Collect all missing dependencies
    let mut downloadables = vec![];
    for (_c, p) in providers.iter() {
        downloadables.append(&mut p.preflight()?);
    }

    // Download those dependencies (serially)
    for d in downloadables {
        if d.exists().await.is_ok() {
            continue;
        }

        println!("{} {}", "Installing".bold().green(), d.name());
        stdout().flush().unwrap();

        // download
        let pb = DownloadPb::new();
        let bytes = d.download(Some(&pb)).await?;
        pb.finished();

        // extract
        let pb = ExtractPb::new();
        d.extract(&bytes, Some(&pb)).await?;
        pb.finished();

        // verify checksums
        d.exists().await?;
    }

    println!("{}", "Launching chains".bold().green());

    // Start up the servers serialy (since they might need to find
    // open ports and avoid clashing with each other)
    let mut servers = vec![];
    let max_name_len = providers
        .iter()
        .map(|(c, _)| c.name().len())
        .max()
        .unwrap_or(0);
    let mut mpb = ServerMpb::new(max_name_len as u8);
    let mut hist = HistoricalMetadata::load();
    for (c, p) in providers {
        let pb = mpb.add(
            c.common().url.to_string(),
            c.name().to_owned(),
            hist.get_server_bootstrap_duration(p.name())
                .unwrap_or_else(|| p.bootstrap_eta()),
        );
        select! {
            _ = pb.auto_update() => {},
            server = p.start() => {
                let server = server?;
                // kill server process immediately if interrupted
                if let Some(pid) = server.pid() {
                    tokio::spawn(async move {
                        tokio::signal::ctrl_c().await.unwrap();
                        kill(pid, SIGTERM).await.unwrap();
                    });
                }
                servers.push((p, server, pb));
            },
        }
    }

    // Wait for all of them to become available
    servers
        .iter_mut()
        .map(|(_p, s, pb)| until_initialized(s, pb))
        .collect::<TryJoinAll<_>>()
        .await?;
    println!("{}", "All servers available".bold().green());

    // Update historical metadata and return servers
    let mut result = vec![];
    for (p, s, pb) in servers {
        result.push(s);
        hist.set_server_bootstrap_duration(p.name(), &pb.duration())
    }
    hist.save()?;
    Ok(result)
}

async fn until_initialized(s: &mut Box<dyn Server>, pb: &ServerPb) -> Result<()> {
    let init = async {
        s.available().await?;
        s.initialize().await
    };
    try_join!(pb.auto_update(), async {
        let res = init.await;
        pb.finished(res.is_ok());
        res
    })?;
    Ok(())
}
