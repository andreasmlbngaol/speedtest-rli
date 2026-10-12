//! Command line interface for testing internet bandwidth using speedtest.net.

mod config;
mod http;
mod results;
mod servers;
mod transfer;

use std::process::ExitCode;

use clap::Parser;

use results::Results;
use servers::Server;

#[derive(Parser, Debug)]
#[command(
    name = "speedtest",
    version = env!("CARGO_PKG_VERSION"),
    about = "Command line interface for testing internet bandwidth using speedtest.net",
    long_about = None,
    disable_version_flag = true
)]
struct Args {
    /// Print version
    #[arg(short = 'v', long = "version", visible_short_alias = 'V', action = clap::ArgAction::Version)]
    version: (),

    /// Do not perform download test
    #[arg(long = "no-download")]
    no_download: bool,

    /// Do not perform upload test
    #[arg(long = "no-upload")]
    no_upload: bool,

    /// Only use a single connection instead of multiple. This simulates a
    /// typical file transfer.
    #[arg(short, long)]
    single: bool,

    /// Display values in bits instead of bytes (default is bytes, MB/s)
    #[arg(short, long)]
    bits: bool,

    /// Deprecated: bytes is now the default. Accepted for compatibility.
    #[arg(long, hide = true)]
    bytes: bool,

    /// Generate and provide a URL to the speedtest.net share results image
    #[arg(short = 'S', long)]
    share: bool,

    /// Suppress verbose output, only show basic information
    #[arg(long)]
    simple: bool,

    /// Suppress verbose output, only show basic information in CSV format
    #[arg(long)]
    csv: bool,

    /// Single character delimiter to use in CSV output. Default ","
    #[arg(long = "csv-delimiter", default_value = ",")]
    csv_delimiter: String,

    /// Print CSV headers
    #[arg(long = "csv-header")]
    csv_header: bool,

    /// Suppress verbose output, only show basic information in JSON format
    #[arg(long)]
    json: bool,

    /// Display a list of speedtest.net servers sorted by distance
    #[arg(short, long)]
    list: bool,

    /// Specify a server ID to test against. Can be supplied multiple times
    #[arg(long, value_name = "ID")]
    server: Vec<i64>,

    /// Exclude a server from selection. Can be supplied multiple times
    #[arg(long, value_name = "ID")]
    exclude: Vec<i64>,

    /// URL of the Speedtest Mini server
    #[arg(long)]
    mini: Option<String>,

    /// Source IP address to bind to
    #[arg(long)]
    source: Option<String>,

    /// HTTP timeout in seconds. Default 10
    #[arg(long, default_value_t = 10.0)]
    timeout: f64,

    /// Use HTTPS instead of HTTP when communicating with speedtest.net servers
    #[arg(long)]
    secure: bool,

    /// Do not pre allocate upload data
    #[arg(long = "no-pre-allocate")]
    no_pre_allocate: bool,

    /// Show verbose debug output
    #[arg(long)]
    debug: bool,
}

fn main() -> ExitCode {
    let args = Args::parse();
    match run(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("ERROR: {msg}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Args) -> Result<(), String> {
    if args.no_download && args.no_upload {
        return Err("Cannot supply both --no-download and --no-upload".to_string());
    }
    if args.csv_delimiter.chars().count() != 1 {
        return Err("--csv-delimiter must be a single character".to_string());
    }

    let delimiter = args.csv_delimiter.chars().next().unwrap();
    if args.csv_header {
        println!("{}", Results::csv_header(delimiter));
    }

    let quiet = args.simple || args.csv || args.json;
    let as_bytes = !args.bits;
    let _ = args.bytes; // deprecated alias; bytes is already the default

    let client = http::client(args.timeout, args.source.as_deref())
        .map_err(|e| format!("Cannot build HTTP client: {e}"))?;

    if !quiet {
        println!("Retrieving speedtest.net configuration...");
    }
    let cfg = config::get_config(&client, args.secure)?;

    if args.debug {
        eprintln!("Config: {cfg:?}");
    }

    if args.list {
        let list = servers::get_servers(&client, &cfg, args.secure, &[], &[])?;
        for s in &list {
            println!(
                "{:>5}) {} ({}, {}) [{:.2} km]",
                s.id, s.sponsor, s.name, s.country, s.d
            );
        }
        return Ok(());
    }

    let isp = cfg.client.get("isp").cloned().unwrap_or_default();
    let ip = cfg.client.get("ip").cloned().unwrap_or_default();
    if !quiet {
        println!("Testing from {isp} ({ip})...");
    }

    let server: Server = if let Some(mini) = &args.mini {
        let list = servers::set_mini_server(&client, mini)?;
        servers::get_best_server(&client, &list)?
    } else {
        if !quiet {
            println!("Retrieving speedtest.net server list...");
        }
        let list = servers::get_servers(&client, &cfg, args.secure, &args.server, &args.exclude)?;
        if !quiet {
            if args.server.len() == 1 {
                println!("Retrieving information for the selected server...");
            } else {
                println!("Selecting best server based on ping...");
            }
        }
        let candidates = servers::closest(&list, 5);
        servers::get_best_server(&client, &candidates)?
    };

    let mut results = Results::new(server, cfg.client.clone());
    results.ping = results.server.latency;

    let threads = if args.single { Some(1) } else { None };

    if !args.no_download {
        if !quiet {
            use std::io::Write;
            print!("Testing download speed");
            std::io::stdout().flush().ok();
        }
        let progress = live_progress("Testing download speed", as_bytes, quiet);
        let (bytes, secs) = transfer::download(&client, &cfg, &results.server, threads, progress);
        results.bytes_received = bytes;
        results.download = if secs > 0.0 {
            bytes as f64 / secs * 8.0
        } else {
            0.0
        };
        if !quiet {
            println!();
        }
    } else if !quiet {
        println!("Skipping download test");
    }

    if !args.no_upload {
        if !quiet {
            use std::io::Write;
            print!("Testing upload speed");
            std::io::stdout().flush().ok();
        }
        let progress = live_progress("Testing upload speed", as_bytes, quiet);
        let (bytes, secs) = transfer::upload(&client, &cfg, &results.server, threads, progress);
        results.bytes_sent = bytes;
        results.upload = if secs > 0.0 {
            bytes as f64 / secs * 8.0
        } else {
            0.0
        };
        if !quiet {
            println!();
        }
    } else if !quiet {
        println!("Skipping upload test");
    }

    if args.debug {
        eprintln!("Results: {}", results.json());
    }

    // Submit results first (if asked) so the report can include the URL.
    if args.share && !args.simple {
        results.share(&client, args.secure)?;
    }

    if args.simple {
        println!(
            "Ping: {} ms\nDownload: {:.2} M{}/s\nUpload: {:.2} M{}/s",
            results.ping,
            mbits(results.download, as_bytes),
            unit(as_bytes),
            mbits(results.upload, as_bytes),
            unit(as_bytes)
        );
    } else if args.csv {
        println!("{}", results.csv(delimiter));
    } else if args.json {
        println!("{}", results.json());
    } else {
        print_report(&results, as_bytes, !args.no_download, !args.no_upload);
    }

    Ok(())
}

/// Print the human-readable results report (the default, non-machine output).
fn print_report(r: &Results, as_bytes: bool, did_download: bool, did_upload: bool) {
    let line = "─".repeat(52);
    println!("{line}");
    println!("  Server    {} ({})", r.server.sponsor, r.server.name);
    println!("  Location  {} [{:.2} km]", r.server.country, r.server.d);
    println!("  Ping      {} ms", r.ping);
    if did_download {
        println!(
            "  Download  {:.2} M{}/s",
            mbits(r.download, as_bytes),
            unit(as_bytes)
        );
    }
    if did_upload {
        println!(
            "  Upload    {:.2} M{}/s",
            mbits(r.upload, as_bytes),
            unit(as_bytes)
        );
    }
    let isp = r.client.get("isp").cloned().unwrap_or_default();
    let ip = r.client.get("ip").cloned().unwrap_or_default();
    println!("  ISP       {isp} ({ip})");
    if let Some(url) = &r.share_url {
        println!("  Share     {url}");
    }
    println!("{line}");
}

/// Megabits (or megabytes) per second from a bits-per-second value.
fn mbits(bits_per_sec: f64, as_bytes: bool) -> f64 {
    let divisor = if as_bytes { 8.0 } else { 1.0 };
    bits_per_sec / 1000.0 / 1000.0 / divisor
}

fn unit(as_bytes: bool) -> &'static str {
    if as_bytes {
        "B"
    } else {
        "b"
    }
}

/// A callback that rewrites the current line with a live speed estimate.
/// `bytes` is the cumulative byte count reported by the transfer thread.
fn live_progress(
    label: &'static str,
    as_bytes: bool,
    quiet: bool,
) -> impl Fn(u64) + Send + Sync + 'static {
    let start = std::time::Instant::now();
    move |bytes: u64| {
        if quiet {
            return;
        }
        let secs = start.elapsed().as_secs_f64();
        let speed = if secs > 0.0 {
            mbits(bytes as f64 / secs * 8.0, as_bytes)
        } else {
            0.0
        };
        use std::io::Write;
        print!("\r{label}  [{speed:>7.2} M{}/s]", unit(as_bytes));
        std::io::stdout().flush().ok();
    }
}
