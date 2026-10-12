//! speedtest.net server list retrieval, distance, and ping selection.

use std::time::Instant;

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};

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

/// One entry from speedtest.net's JSON server API.
#[derive(Deserialize)]
struct ApiServer {
    #[serde(default)]
    id: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    country: String,
    #[serde(default)]
    sponsor: String,
    #[serde(default)]
    lat: String,
    #[serde(default)]
    lon: String,
    #[serde(default)]
    distance: f64,
}

/// Fetch and parse servers, applying `only` / `exclude` filters.
///
/// Uses speedtest.net's JSON API (`/api/js/servers`), which geolocates on the
/// server side and returns the nearest servers. The old
/// `speedtest-servers-static.php` XML feed is dead: it returns a fixed set of
/// 10 Singapore servers regardless of the client's location.
pub fn get_servers(
    client: &Client,
    cfg: &Config,
    secure: bool,
    only: &[i64],
    exclude: &[i64],
) -> Result<Vec<Server>, String> {
    let mut url = format!(
        "{}://www.speedtest.net/api/js/servers?engine=js&limit=100",
        http::scheme(secure)
    );
    if !only.is_empty() {
        let ids = only
            .iter()
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(",");
        url.push_str(&format!("&server_ids={ids}"));
    }

    let body = client
        .get(&url)
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.text())
        .map_err(|e| format!("Cannot retrieve speedtest server list: {e}"))?;

    parse_servers(&body, cfg, only, exclude)
}

fn parse_servers(
    json: &str,
    cfg: &Config,
    only: &[i64],
    exclude: &[i64],
) -> Result<Vec<Server>, String> {
    let list: Vec<ApiServer> = serde_json::from_str(json)
        .map_err(|e| format!("Malformed speedtest.net server list: {e}"))?;

    let mut servers = Vec::new();
    for s in list {
        let id: i64 = match s.id.parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        if !only.is_empty() && !only.contains(&id) {
            continue;
        }
        if cfg.ignore_servers.contains(&id) || exclude.contains(&id) {
            continue;
        }
        servers.push(Server {
            id: s.id,
            sponsor: s.sponsor,
            name: s.name,
            country: s.country,
            url: s.url,
            lat: s.lat.parse().unwrap_or(0.0),
            lon: s.lon.parse().unwrap_or(0.0),
            // Server-side distance, computed by speedtest.net from our real IP.
            d: s.distance,
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

    fn cfg() -> Config {
        Config {
            client: Default::default(),
            ignore_servers: vec![],
            upload_sizes: vec![],
            download_sizes: vec![],
            upload_count: 0,
            download_count: 0,
            upload_threads: 1,
            download_threads: 1,
            upload_length: 0,
            download_length: 0,
            upload_max: 0,
        }
    }

    #[test]
    fn parses_json_api_and_sorts_by_distance() {
        let json = r#"[
            {"id":"2","url":"http://b/upload.php","name":"Far","country":"ID",
             "sponsor":"B","lat":"1.0","lon":"2.0","distance":900.0},
            {"id":"1","url":"http://a/upload.php","name":"Near","country":"ID",
             "sponsor":"A","lat":"3.0","lon":"4.0","distance":5.0}
        ]"#;
        let servers = parse_servers(json, &cfg(), &[], &[]).unwrap();
        assert_eq!(servers.len(), 2);
        assert_eq!(servers[0].id, "1");
        assert_eq!(servers[0].name, "Near");
        assert_eq!(servers[0].d, 5.0);
        assert_eq!(servers[1].id, "2");
    }

    #[test]
    fn excludes_ignored_and_only_keeps_selected() {
        let json = r#"[
            {"id":"1","url":"u","name":"A","country":"ID","sponsor":"A","lat":"0","lon":"0","distance":1.0},
            {"id":"2","url":"u","name":"B","country":"ID","sponsor":"B","lat":"0","lon":"0","distance":2.0}
        ]"#;
        let servers = parse_servers(json, &cfg(), &[2], &[]).unwrap();
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].id, "2");

        let mut c = cfg();
        c.ignore_servers = vec![1];
        let servers = parse_servers(json, &c, &[], &[]).unwrap();
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].id, "2");
    }
}
