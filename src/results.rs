//! Result data, machine-readable output, and the share-image submission.

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use reqwest::blocking::Client;
use serde::Serialize;
use serde_json::json;

use crate::http;
use crate::servers::Server;

/// Salt baked into the speedtest.net share API hash.
const SHARE_SALT: &str = "297aae72";

#[derive(Debug, Clone, Serialize)]
pub struct Results {
    /// Bits per second.
    pub download: f64,
    /// Bits per second.
    pub upload: f64,
    /// Milliseconds.
    pub ping: f64,
    pub server: Server,
    pub client: HashMap<String, String>,
    pub timestamp: String,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    #[serde(skip)]
    pub share_url: Option<String>,
}

impl Results {
    pub fn new(server: Server, client: HashMap<String, String>) -> Self {
        Results {
            download: 0.0,
            upload: 0.0,
            ping: 0.0,
            server,
            client,
            timestamp: format!("{}Z", iso8601_utc()),
            bytes_sent: 0,
            bytes_received: 0,
            share_url: None,
        }
    }

    /// The result as a JSON value matching upstream's `dict()` layout.
    pub fn to_value(&self) -> serde_json::Value {
        json!({
            "download": self.download,
            "upload": self.upload,
            "ping": self.ping,
            "server": self.server,
            "timestamp": self.timestamp,
            "bytes_sent": self.bytes_sent,
            "bytes_received": self.bytes_received,
            "share": self.share_url,
            "client": self.client,
        })
    }

    pub fn json(&self) -> String {
        self.to_value().to_string()
    }

    pub fn csv_header(delimiter: char) -> String {
        let row = [
            "Server ID", "Sponsor", "Server Name", "Timestamp", "Distance", "Ping",
            "Download", "Upload", "Share", "IP Address",
        ];
        row.join(&delimiter.to_string())
    }

    pub fn csv(&self, delimiter: char) -> String {
        let row = [
            self.server.id.clone(),
            self.server.sponsor.clone(),
            self.server.name.clone(),
            self.timestamp.clone(),
            format!("{}", self.server.d),
            format!("{}", self.ping),
            format!("{}", self.download),
            format!("{}", self.upload),
            self.share_url.clone().unwrap_or_default(),
            self.client.get("ip").cloned().unwrap_or_default(),
        ];
        row.join(&delimiter.to_string())
    }

    /// POST results to the speedtest.net API and return a share image URL.
    pub fn share(&mut self, client: &Client, secure: bool) -> Result<String, String> {
        if let Some(url) = &self.share_url {
            return Ok(url.clone());
        }

        let download = (self.download / 1000.0).round() as i64;
        let upload = (self.upload / 1000.0).round() as i64;
        let ping = self.ping.round() as i64;

        let hash = md5::compute(format!("{ping}-{upload}-{download}-{SHARE_SALT}"));

        let params = [
            ("recommendedserverid", self.server.id.clone()),
            ("ping", ping.to_string()),
            ("screenresolution", String::new()),
            ("promo", String::new()),
            ("download", download.to_string()),
            ("screendpi", String::new()),
            ("upload", upload.to_string()),
            ("testmethod", "http".to_string()),
            ("hash", format!("{hash:x}")),
            ("touchscreen", "none".to_string()),
            ("startmode", "pingselect".to_string()),
            ("accuracy", "1".to_string()),
            ("bytesreceived", self.bytes_received.to_string()),
            ("bytessent", self.bytes_sent.to_string()),
            ("serverid", self.server.id.clone()),
        ];

        let url = format!("{}://www.speedtest.net/api/api.php", http::scheme(secure));
        let resp = client
            .post(&url)
            .header("Referer", "http://c.speedtest.net/flash/speedtest.swf")
            .form(&params)
            .send()
            .map_err(|e| format!("Could not submit results to speedtest.net: {e}"))?;

        let body = resp
            .text()
            .map_err(|e| format!("Could not submit results to speedtest.net: {e}"))?;

        let resultid = parse_result_id(&body)
            .ok_or("Could not submit results to speedtest.net")?;
        let share = format!("http://www.speedtest.net/result/{resultid}.png");
        self.share_url = Some(share.clone());
        Ok(share)
    }
}

fn parse_result_id(body: &str) -> Option<String> {
    for pair in body.split('&') {
        if let Some(v) = pair.strip_prefix("resultid=") {
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    None
}

/// ISO-8601 UTC timestamp (`YYYY-MM-DDTHH:MM:SS.ffffff`) with no timezone
/// suffix, matching Python's `datetime.utcnow().isoformat()`.
fn iso8601_utc() -> String {
    let dur = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = dur.as_secs() as i64;
    let micros = dur.subsec_micros();

    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400);
    let (h, m, s) = (tod / 3600, (tod % 3600) / 60, tod % 60);
    let (y, mo, d) = civil_from_days(days);

    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}.{micros:06}")
}

/// Days since 1970-01-01 to (year, month, day). Howard Hinnant's algorithm.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_is_1970() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_723), (2024, 1, 1)); // 2024-01-01
    }

    #[test]
    fn parses_result_id() {
        assert_eq!(parse_result_id("resultid=12345&foo=bar").as_deref(), Some("12345"));
        assert_eq!(parse_result_id("foo=bar"), None);
    }
}
