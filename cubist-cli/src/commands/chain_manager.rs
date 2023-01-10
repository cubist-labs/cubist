use eyre::Result;

use cubist_config::network::NetworkProfile;
use cubist_localchains::provider::Server;

pub async fn start(cfg: &NetworkProfile) -> Result<Vec<Box<dyn Server>>> {
    Ok(cubist_localchains::start(cfg).await?)
}
