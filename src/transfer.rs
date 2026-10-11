//! Multithreaded download and upload tests.

use std::io::Read;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
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
fn run_pool<F>(jobs: usize, max_threads: usize, work: F) -> u64
where
    F: Fn(usize) -> u64 + Send + Sync + 'static,
{
    let threads = max_threads.max(1).min(jobs.max(1));
    let next = Arc::new(AtomicUsize::new(0));
    let work = Arc::new(work);

    let handles: Vec<_> = (0..threads)
        .map(|_| {
            let next = Arc::clone(&next);
            let work = Arc::clone(&work);
            std::thread::spawn(move || {
                let mut total = 0u64;
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= jobs {
                        break;
                    }
                    total += work(i);
                }
                total
            })
        })
        .collect();

    handles.into_iter().filter_map(|h| h.join().ok()).sum()
}

/// Spawn a thread that calls `progress(cumulative_bytes)` every 100 ms until
/// the returned flag is set, then makes one final call. Keeps the readout
/// live even while a single long request is in flight.
fn with_ticker(
    total: Arc<AtomicU64>,
    progress: impl Fn(u64) + Send + Sync + 'static,
) -> (Arc<AtomicBool>, std::thread::JoinHandle<()>) {
    let done = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&done);
    let handle = std::thread::spawn(move || {
        while !flag.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(100));
            progress(total.load(Ordering::Relaxed));
        }
        progress(total.load(Ordering::Relaxed));
    });
    (done, handle)
}

/// Download test. Returns bytes received and elapsed seconds.
pub fn download(
    client: &Client,
    cfg: &Config,
    server: &Server,
    threads: Option<usize>,
    progress: impl Fn(u64) + Send + Sync + 'static,
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
    let total = Arc::new(AtomicU64::new(0));

    let jobs = urls.len();
    let client = client.clone();
    let counter = Arc::clone(&total);
    let (done, ticker) = with_ticker(Arc::clone(&total), progress);
    run_pool(
        jobs,
        max_threads,
        move |i| {
            let url = urls[i].clone();
            let client = client.clone();
            let counter = Arc::clone(&counter);
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
                        Ok(n) => {
                            got += n as u64;
                            counter.fetch_add(n as u64, Ordering::Relaxed);
                        }
                        Err(_) => break,
                    }
                }
            }
            got
        },
    );
    done.store(true, Ordering::Relaxed);
    ticker.join().ok();

    let elapsed = start.elapsed().as_secs_f64();
    (total.load(Ordering::Relaxed), elapsed)
}

/// Upload test. Returns bytes sent and elapsed seconds.
pub fn upload(
    client: &Client,
    cfg: &Config,
    server: &Server,
    threads: Option<usize>,
    progress: impl Fn(u64) + Send + Sync + 'static,
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
    let total = Arc::new(AtomicU64::new(0));

    let client = client.clone();
    let counter = Arc::clone(&total);
    let (done, ticker) = with_ticker(Arc::clone(&total), progress);
    run_pool(
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
                Ok(_) => {
                    counter.fetch_add(size, Ordering::Relaxed);
                    size
                }
                Err(_) => 0,
            }
        },
    );
    done.store(true, Ordering::Relaxed);
    ticker.join().ok();

    let elapsed = start.elapsed().as_secs_f64();
    (total.load(Ordering::Relaxed), elapsed)
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
