mod load_balancer;
mod service_discovery;

pub use load_balancer::*;
pub use service_discovery::*;

#[cfg(feature = "api-client")]
mod feign_client;

#[cfg(feature = "api-client")]
pub use feign_client::*;