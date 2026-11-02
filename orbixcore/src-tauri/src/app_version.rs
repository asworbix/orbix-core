use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct AppVersion {
    pub version: String,
    pub build: String,
}

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn current() -> AppVersion {
    AppVersion {
        version: APP_VERSION.to_string(),
        build: format!("{}-{}-{}", APP_VERSION, std::env::consts::OS, std::env::consts::ARCH),
    }
}
