//! Multithreaded download and upload tests.

use std::io::Read;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use reqwest::blocking::Client;

use crate::config::Config;
use crate::servers::Server;

fn dirname(url: &str) -> String {
    match url.rfind('/') {
        Some(i) => url[..i].to_string(),
        None => url.to_string(),
    }
}

/// Run `work` across at most `max_threads` workers, returning the summed
/// `u64` results. Work items are pulled from a shared atomic counter.
/// `progress` runs once per completed job.
fn run_pool<F, P>(jobs: usize, max_threads: usize, work: F, progress: P) -> u64
where
    F: Fn(usize) -> u64 + Send + Sync + 'static,
    P: Fn() + Send + Sync + 'static,
{
    let threads = max_threads.max(1).min(jobs.max(1));
    let next = Arc::new(AtomicUsize::new(0));
    let work = Arc::new(work);
    let progress = Arc::new(progress);
    let (tx, rx) = mpsc::channel();

    let handles: Vec<_> = (0..threads)
        .map(|_| {
            let next = Arc::clone(&next);
            let work = Arc::clone(&work);
            let progress = Arc::clone(&progress);
            let tx = tx.clone();
            std::thread::spawn(move || loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                if i >= jobs {
                    break;
                }
                let result = work(i);
                progress();
                tx.send(result).ok();
            })
        })
        .collect();
    drop(tx);

    let total: u64 = rx.iter().sum();
    for h in handles {
        h.join().ok();
    }
    total
}

/// Download test. Returns bytes received and elapsed seconds.
pub fn download(
    client: &Client,
    cfg: &Config,
    server: &Server,
    threads: Option<usize>,
    progress: impl Fn() + Send + Sync + 'static,
) -> (u64, f64) {
    let base = dirname(&server.url);
    let mut urls = Vec::new();
    for size in &cfg.download_sizes {
        for _ in 0..cfg.download_count {
            urls.push(format!("{base}/random{size}x{size}.jpg"));
        }
    }

    let max_threads = threads.unwrap_or(cfg.download_threads);
    let window = Duration::from_secs(cfg.download_length);
    let start = Instant::now();

    let jobs = urls.len();
    let client = client.clone();
    let bytes = run_pool(
        jobs,
        max_threads,
        move |i| {
            let url = urls[i].clone();
            let client = client.clone();
            let start = start;
            let window = window;
            let mut got = 0u64;
            if start.elapsed() > window {
                return 0;
            }
            if let Ok(resp) = client.get(&url).send() {
                let mut resp = resp;
                let mut buf = [0u8; 10240];
                loop {
                    if start.elapsed() > window {
                        break;
                    }
                    match resp.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => got += n as u64,
                        Err(_) => break,
                    }
                }
            }
            got
        },
        progress,
    );

    let elapsed = start.elapsed().as_secs_f64();
    (bytes, elapsed)
}

/// Upload test. Returns bytes sent and elapsed seconds.
pub fn upload(
    client: &Client,
    cfg: &Config,
    server: &Server,
    threads: Option<usize>,
    progress: impl Fn() + Send + Sync + 'static,
) -> (u64, f64) {
    let mut sizes = Vec::new();
    for size in &cfg.upload_sizes {
        for _ in 0..cfg.upload_count {
            sizes.push(*size);
        }
    }
    let jobs = cfg.upload_max as usize;

    let max_threads = threads.unwrap_or(cfg.upload_threads);
    let window = Duration::from_secs(cfg.upload_length);
    let url = server.url.clone();
    let start = Instant::now();

    let client = client.clone();
    let bytes = run_pool(
        jobs,
        max_threads,
        move |i| {
            let Some(&size) = sizes.get(i) else {
                return 0;
            };
            if start.elapsed() > window {
                return 0;
            }
            let body = upload_body(size);
            match client.post(&url).body(body).send() {
                Ok(_) => size,
                Err(_) => 0,
            }
        },
        progress,
    );

    let elapsed = start.elapsed().as_secs_f64();
    (bytes, elapsed)
}

/// Build the `content1=<repeated chars>` payload of exactly `size` bytes.
fn upload_body(size: u64) -> Vec<u8> {
    const CHARS: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let prefix = b"content1=";
    let mut body = Vec::with_capacity(size as usize);
    body.extend_from_slice(prefix);
    while (body.len() as u64) < size {
        let take = (size as usize - body.len()).min(CHARS.len());
        body.extend_from_slice(&CHARS[..take]);
    }
    body.truncate(size as usize);
    body
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upload_body_exact_size() {
        for size in [32768u64, 65536, 131072] {
            let body = upload_body(size);
            assert_eq!(body.len() as u64, size);
            assert!(body.starts_with(b"content1="));
        }
    }
}
