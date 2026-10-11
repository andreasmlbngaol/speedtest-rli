//! Integration tests: run the real binary and check its observable behaviour.
//!
//! These deliberately avoid anything that touches the network, so they are
//! fast and deterministic. Only argument handling and pre-flight validation
//! are covered here.

use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_speedtest-rli");

fn run(args: &[&str]) -> std::process::Output {
    Command::new(BIN)
        .args(args)
        .output()
        .expect("failed to run speedtest-rli")
}

#[test]
fn version_flag_prints_name_and_version() {
    let out = run(&["--version"]);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("speedtest-rli"), "got: {stdout}");
    assert!(stdout.contains(env!("CARGO_PKG_VERSION")), "got: {stdout}");
}

#[test]
fn help_lists_key_flags() {
    let out = run(&["--help"]);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    for flag in ["--json", "--csv", "--list", "--server", "--share"] {
        assert!(stdout.contains(flag), "missing {flag} in help");
    }
}

#[test]
fn csv_header_alone_prints_header_without_network() {
    // --csv-header on its own must print the header and then fail fast on the
    // missing config fetch; the header line is what we assert on.
    let out = run(&["--csv-header", "--timeout", "0.001"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Server ID,Sponsor,Server Name"),
        "got: {stdout}"
    );
}

#[test]
fn both_no_download_and_no_upload_is_rejected() {
    let out = run(&["--no-download", "--no-upload"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("Cannot supply both --no-download and --no-upload"),
        "got: {stderr}"
    );
}

#[test]
fn multi_character_csv_delimiter_is_rejected() {
    let out = run(&["--csv-delimiter", "ab"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--csv-delimiter must be a single character"),
        "got: {stderr}"
    );
}
