//! speedtest.net server list retrieval, distance, and ping selection.

use std::time::Instant;

use reqwest::blocking::Client;
use serde::Serialize;

use crate::config::Config;
use crate::http;

#[derive(Debug, Clone, Default, Serialize)]
pub struct Server {
    pub id: String,
    pub sponsor: String,
    pub name: String,
    pub country: String,
    pub url: String,
    pub lat: f64,
    pub lon: f64,
    pub d: f64,
    pub latency: f64,
}

/// Haversine distance in km between two (lat, lon) points.
pub fn distance(origin: (f64, f64), destination: (f64, f64)) -> f64 {
    let (lat1, lon1) = origin;
    let (lat2, lon2) = destination;
    let radius = 6371.0_f64;

    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
    radius * c
}

const SERVER_URLS: [&str; 4] = [
    "://www.speedtest.net/speedtest-servers-static.php",
    "http://c.speedtest.net/speedtest-servers-static.php",
    "://www.speedtest.net/speedtest-servers.php",
    "http://c.speedtest.net/speedtest-servers.php",
];

/// Fetch and parse servers, applying `only` / `exclude` filters.
pub fn get_servers(
    client: &Client,
    cfg: &Config,
    secure: bool,
    only: &[i64],
    exclude: &[i64],
) -> Result<Vec<Server>, String> {
    let mut last_err = String::from("Cannot retrieve speedtest server list");

    for raw in SERVER_URLS {
        let url = if raw.starts_with(':') {
            format!("{}{}", http::scheme(secure), raw)
        } else {
            raw.to_string()
        };
        let url = format!("{url}?threads={}", cfg.download_threads);

        let body = match client.get(&url).send().and_then(|r| r.error_for_status()).and_then(|r| r.text()) {
            Ok(b) => b,
            Err(e) => {
                last_err = format!("Cannot retrieve speedtest server list: {e}");
                continue;
            }
        };

        match parse_servers(&body, cfg, only, exclude) {
            Ok(servers) => return Ok(servers),
            Err(e) => last_err = e,
        }
    }

    Err(last_err)
}

fn parse_servers(
    xml: &str,
    cfg: &Config,
    only: &[i64],
    exclude: &[i64],
) -> Result<Vec<Server>, String> {
    let doc = roxmltree::Document::parse(xml)
        .map_err(|e| format!("Malformed speedtest.net server list: {e}"))?;

    let mut servers = Vec::new();
    for node in doc.descendants().filter(|n| n.has_tag_name("server")) {
        let get = |k: &str| node.attribute(k).unwrap_or("").to_string();
        let id: i64 = match get("id").parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        if !only.is_empty() && !only.contains(&id) {
            continue;
        }
        if cfg.ignore_servers.contains(&id) || exclude.contains(&id) {
            continue;
        }
        let (lat, lon) = match (get("lat").parse::<f64>(), get("lon").parse::<f64>()) {
            (Ok(la), Ok(lo)) => (la, lo),
            _ => continue,
        };
        servers.push(Server {
            id: id.to_string(),
            sponsor: get("sponsor"),
            name: get("name"),
            country: get("country"),
            url: get("url"),
            lat,
            lon,
            d: distance(cfg.lat_lon, (lat, lon)),
            latency: 0.0,
        });
    }

    if (!only.is_empty() || !exclude.is_empty()) && servers.is_empty() {
        return Err("No matched servers".to_string());
    }

    servers.sort_by(|a, b| a.d.partial_cmp(&b.d).unwrap_or(std::cmp::Ordering::Equal));
    Ok(servers)
}

/// The `limit` geographically closest servers.
pub fn closest(servers: &[Server], limit: usize) -> Vec<Server> {
    servers.iter().take(limit).cloned().collect()
}

fn dirname(url: &str) -> String {
    match url.rfind('/') {
        Some(i) => url[..i].to_string(),
        None => url.to_string(),
    }
}

/// Ping candidate servers and return the one with the lowest latency.
pub fn get_best_server(client: &Client, servers: &[Server]) -> Result<Server, String> {
    let mut best: Option<(f64, Server)> = None;

    for server in servers {
        let base = dirname(&server.url);
        let stamp = http::now_millis();
        let mut cum = Vec::with_capacity(3);

        for i in 0..3 {
            let url = format!("{base}/latency.txt?x={stamp}.{i}");
            let start = Instant::now();
            let ok = client
                .get(&url)
                .send()
                .ok()
                .filter(|r| r.status().is_success())
                .and_then(|r| r.bytes().ok())
                .map(|b| b.starts_with(b"test=test"))
                .unwrap_or(false);
            if ok {
                cum.push(start.elapsed().as_secs_f64());
            } else {
                cum.push(3600.0);
            }
        }

        // Python: round((sum(cum) / 6) * 1000.0, 3)
        let avg = ((cum.iter().sum::<f64>() / 6.0 * 1000.0) * 1000.0).round() / 1000.0;
        if best.as_ref().map(|(b, _)| avg < *b).unwrap_or(true) {
            best = Some((avg, server.clone()));
        }
    }

    let (latency, mut server) = best.ok_or("Unable to connect to servers to test latency.")?;
    server.latency = latency;
    Ok(server)
}

/// Resolve a Speedtest Mini server URL into a single-server list.
pub fn set_mini_server(client: &Client, server: &str) -> Result<Vec<Server>, String> {
    let parsed = reqwest::Url::parse(server).map_err(|_| {
        format!("Invalid Speedtest Mini Server: {server}")
    })?;

    // If the URL has a file component, use its directory.
    let path = parsed.path();
    let base = match path.rfind('.') {
        Some(i) if !path[i..].contains('/') => &server[..server.rfind('/').unwrap_or(server.len())],
        _ => server,
    };

    let text = client
        .get(base)
        .send()
        .and_then(|r| r.text())
        .map_err(|_| format!("Failed to connect to {server}"))?;

    let mut extension = find_upload_extension(&text);

    if extension.is_none() {
        for ext in ["php", "asp", "aspx", "jsp"] {
            let url = format!("{}/speedtest/upload.{ext}", base.trim_end_matches('/'));
            if let Ok(resp) = client.get(&url).send() {
                if resp.status().is_success() {
                    if let Ok(body) = resp.text() {
                        let body = body.trim();
                        if body.lines().count() == 1 && body.starts_with("size=") {
                            extension = Some(ext.to_string());
                            break;
                        }
                    }
                }
            }
        }
    }

    let extension = extension
        .ok_or_else(|| format!("Invalid Speedtest Mini Server: {server}"))?;

    let host = parsed.host_str().unwrap_or("").to_string();
    Ok(vec![Server {
        id: "0".to_string(),
        sponsor: "Speedtest Mini".to_string(),
        name: host,
        url: format!("{}/speedtest/upload.{extension}", base.trim_end_matches('/')),
        d: 0.0,
        latency: 0.0,
        ..Default::default()
    }])
}

/// Find `upload_?[Ee]xtension: "([^"]+)"` in a Mini server landing page.
fn find_upload_extension(text: &str) -> Option<String> {
    let marker = "xtension: \"";
    let idx = text.find(marker)?;
    let rest = &text[idx + marker.len()..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_zero_for_same_point() {
        assert_eq!(distance((1.0, 2.0), (1.0, 2.0)), 0.0);
    }

    #[test]
    fn distance_known_pair() {
        // Jakarta (-6.2, 106.8) to Bandung (-6.9, 107.6) ~ 118 km
        let d = distance((-6.2, 106.8), (-6.9, 107.6));
        assert!((d - 118.0).abs() < 5.0, "got {d}");
    }
}
