use crate::provider::{Provider, RemoteProvider};
use crate::{error::Result, UrlExt};
use cubist_config::{
    network::{CommonConfig, EndpointConfig, NetworkProfile},
    Target,
};

pub fn endpoints(profile: &NetworkProfile) -> impl Iterator<Item = EndpointConfig> {
    [
        profile.get(Target::Ethereum),
        profile.get(Target::Polygon),
        profile.get(Target::Avalanche),
        profile.get(Target::AvaSubnet),
    ]
    .into_iter()
    .flatten()
}

impl From<EndpointConfig> for Box<dyn Config> {
    fn from(endpoint: EndpointConfig) -> Self {
        match endpoint {
            EndpointConfig::Eth(c) => Box::new(c),
            EndpointConfig::Poly(c) => Box::new(c),
            EndpointConfig::Ava(c) => Box::new(c),
            EndpointConfig::AvaSub(c) => Box::new(c.with_default_subnet()),
        }
    }
}

impl TryFrom<EndpointConfig> for Box<dyn Provider> {
    type Error = crate::Error;

    fn try_from(endpoint: EndpointConfig) -> Result<Self, Self::Error> {
        let config: Box<dyn Config> = endpoint.into();
        config.provider()
    }
}

pub(crate) fn configs(profile: &NetworkProfile) -> impl Iterator<Item = Box<dyn Config>> {
    endpoints(profile).map(From::from)
}

/// Configs are the result of user entry and may have some strange options
pub(crate) trait Config {
    /// Name of the target chain
    fn name(&self) -> &str;

    /// Returns common configuration
    fn common(&self) -> CommonConfig;

    /// Creates a provider from this configuration
    fn provider(&self) -> Result<Box<dyn Provider>> {
        let cfg = self.common();
        let is_local = cfg.url.is_loopback()?;
        if is_local && cfg.autostart {
            self.local_provider()
        } else {
            Ok(Box::new(RemoteProvider {
                name: format!("(remote) {}", self.name()),
                config: cfg,
            }))
        }
    }

    /// Returns a provider which starts a local node when its
    /// [`Provider::start`] method is called.
    ///
    /// Only allowed when [`CommonConfig::url`] is a loopback address.
    ///
    /// Should not be called directly; call [`Config::provider`] instead.
    fn local_provider(&self) -> Result<Box<dyn Provider>>;
}
