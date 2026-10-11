//! HTTP client setup and request helpers.

use std::net::IpAddr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use reqwest::blocking::Client;

pub fn scheme(secure: bool) -> &'static str {
    if secure {
        "https"
    } else {
        "http"
    }
}

/// Build a client. `source` optionally binds the outgoing socket to a local
/// address; `timeout` is the per-request HTTP timeout in seconds.
pub fn client(timeout: f64, source: Option<&str>) -> reqwest::Result<Client> {
    let mut builder = Client::builder()
        .timeout(Duration::from_secs_f64(timeout))
        .gzip(true)
        .user_agent(user_agent());

    if let Some(src) = source {
        if let Ok(ip) = src.parse::<IpAddr>() {
            builder = builder.local_address(ip);
        }
    }

    builder.build()
}

pub fn user_agent() -> String {
    format!(
        "Mozilla/5.0 ({}; U; {}; en-us) speedtest-rli/{}",
        std::env::consts::OS,
        std::env::consts::ARCH,
        env!("CARGO_PKG_VERSION"),
    )
}

pub fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}
