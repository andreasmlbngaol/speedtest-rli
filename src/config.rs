//! speedtest.net configuration retrieval and parsing.

use std::collections::HashMap;

use reqwest::blocking::Client;

use crate::http;

pub const UP_SIZES: [u64; 7] = [32768, 65536, 131072, 262144, 524288, 1048576, 7340032];
pub const DOWN_SIZES: [u64; 10] = [350, 500, 750, 1000, 1500, 2000, 2500, 3000, 3500, 4000];

#[derive(Debug, Clone)]
pub struct Config {
    pub client: HashMap<String, String>,
    pub ignore_servers: Vec<i64>,
    pub upload_sizes: Vec<u64>,
    pub download_sizes: Vec<u64>,
    pub upload_count: u64,
    pub download_count: u64,
    pub upload_threads: usize,
    pub download_threads: usize,
    pub upload_length: u64,
    pub download_length: u64,
    pub upload_max: u64,
}

pub fn get_config(client: &Client, secure: bool) -> Result<Config, String> {
    let url = format!(
        "{}://www.speedtest.net/speedtest-config.php",
        http::scheme(secure)
    );
    let body = client
        .get(&url)
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.text())
        .map_err(|e| format!("Cannot retrieve speedtest configuration: {e}"))?;

    parse_config(&body)
}

fn attrs<'a>(doc: &'a roxmltree::Document, tag: &str) -> HashMap<String, String> {
    doc.descendants()
        .find(|n| n.has_tag_name(tag))
        .map(|n| {
            n.attributes()
                .map(|a| (a.name().to_string(), a.value().to_string()))
                .collect()
        })
        .unwrap_or_default()
}

fn parse_config(xml: &str) -> Result<Config, String> {
    let doc = roxmltree::Document::parse(xml)
        .map_err(|e| format!("Malformed speedtest.net configuration: {e}"))?;

    let server_config = attrs(&doc, "server-config");
    let download = attrs(&doc, "download");
    let upload = attrs(&doc, "upload");
    let client = attrs(&doc, "client");

    let ignore_servers = server_config
        .get("ignoreids")
        .map(|s| s.split(',').filter(|i| !i.is_empty()).filter_map(|i| i.parse().ok()).collect())
        .unwrap_or_default();

    let ratio: usize = upload.get("ratio").and_then(|v| v.parse().ok()).unwrap_or(1);
    let upload_max: u64 = upload.get("maxchunkcount").and_then(|v| v.parse().ok()).unwrap_or(0);

    let upload_sizes: Vec<u64> = UP_SIZES[ratio.saturating_sub(1).min(UP_SIZES.len() - 1)..].to_vec();
    let download_sizes = DOWN_SIZES.to_vec();

    let size_count = upload_sizes.len() as u64;
    let upload_count = (upload_max + size_count - 1) / size_count.max(1);

    let download_count: u64 = download.get("threadsperurl").and_then(|v| v.parse().ok()).unwrap_or(1);
    let upload_threads = upload.get("threads").and_then(|v| v.parse().ok()).unwrap_or(1);
    let download_threads = server_config
        .get("threadcount")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(1)
        * 2;
    let upload_length = upload.get("testlength").and_then(|v| v.parse().ok()).unwrap_or(10);
    let download_length = download.get("testlength").and_then(|v| v.parse().ok()).unwrap_or(10);

    Ok(Config {
        client,
        ignore_servers,
        upload_sizes,
        download_sizes,
        upload_count,
        download_count,
        upload_threads,
        download_threads,
        upload_length,
        download_length,
        upload_max: upload_count * size_count,
    })
}
