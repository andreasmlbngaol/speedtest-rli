speedtest-rli
=============

Command line interface for testing internet bandwidth using
speedtest.net, written in Rust.

This is a Rust rewrite of the original Python
`speedtest-cli <https://github.com/sivel/speedtest-cli>`_ by Matt Martz.
It aims for feature parity with the Python CLI while shipping as a single
native binary with no runtime dependency. See ``NOTICE`` for attribution.

Installation
------------

Windows
~~~~~~~

Open **PowerShell** (press ``Win``, type ``powershell``, press Enter) and
paste this one line:

::

    irm https://github.com/andreasmlbngaol/speedtest-rli/releases/latest/download/install.ps1 | iex

That downloads the program and puts it on your ``PATH`` for you. No admin
rights needed. When it finishes, open a **new** PowerShell or Command Prompt
window and type:

::

    speedtest-rli

Linux / macOS
~~~~~~~~~~~~~

Open a terminal and paste this one line:

::

    curl -fsSL https://github.com/andreasmlbngaol/speedtest-rli/releases/latest/download/install.sh | sh

That installs to ``~/.local/bin`` (no ``sudo``). If that folder is not on your
``PATH`` yet, the script tells you the one line to add.

Manual download
~~~~~~~~~~~~~~~

Grab the file for your platform from the
`releases page <https://github.com/andreasmlbngaol/speedtest-rli/releases/latest>`_:

- Linux x86_64: ``speedtest-rli-linux-x86_64``
- Windows x86_64: ``speedtest-rli-windows-x86_64.exe``
- macOS Apple Silicon: ``speedtest-rli-macos-aarch64``

On Linux/macOS::

    chmod +x speedtest-rli-linux-x86_64
    sudo mv speedtest-rli-linux-x86_64 /usr/local/bin/speedtest-rli

On Windows, the ``.exe`` runs by double-clicking; to call it as
``speedtest-rli`` from any terminal, put it in a folder that is on your
``PATH``.

From source
~~~~~~~~~~~

Requires a stable Rust toolchain.

::

    cargo build --release

The binary lands at ``target/release/speedtest-rli``. Install it with::

    cargo install --path .

Or install straight from GitHub::

    cargo install --git https://github.com/andreasmlbngaol/speedtest-rli


Usage
-----

::

    $ speedtest-rli --help
    Command line interface for testing internet bandwidth using speedtest.net

    Usage: speedtest-rli [OPTIONS]

    Options:
      -v, --version                        Print version [alias: -V]
          --no-download                    Do not perform download test
          --no-upload                      Do not perform upload test
      -s, --single                         Only use a single connection instead of multiple
      -b, --bits                           Display values in bits instead of bytes (default is bytes, MB/s)
      -S, --share                          Generate and provide a URL to the speedtest.net share results image
          --simple                         Suppress verbose output, only show basic information
          --csv                            Suppress verbose output, only show basic information in CSV format
          --csv-delimiter <CSV_DELIMITER>  Single character delimiter to use in CSV output. Default "," [default: ,]
          --csv-header                     Print CSV headers
          --json                           Suppress verbose output, only show basic information in JSON format
      -l, --list                           Display a list of speedtest.net servers sorted by distance
          --server <ID>                    Specify a server ID to test against. Can be supplied multiple times
          --exclude <ID>                   Exclude a server from selection. Can be supplied multiple times
          --mini <MINI>                    URL of the Speedtest Mini server
          --source <SOURCE>                Source IP address to bind to
          --timeout <TIMEOUT>              HTTP timeout in seconds. Default 10 [default: 10]
          --secure                         Use HTTPS instead of HTTP when communicating with speedtest.net servers
          --no-pre-allocate                Do not pre allocate upload data
          --debug                          Show verbose debug output
      -h, --help                           Print help

Examples
--------

::

    # Full test
    speedtest-rli

    # Machine-readable output
    speedtest-rli --json
    speedtest-rli --csv
    speedtest-rli --simple

    # Test against a specific server
    speedtest-rli --server 1234 --secure

    # List nearby servers
    speedtest-rli --list

Layout
------

- ``src/main.rs`` — CLI parsing and orchestration
- ``src/config.rs`` — speedtest.net configuration fetch/parse
- ``src/servers.rs`` — server list, distance, ping selection
- ``src/transfer.rs`` — multithreaded download/upload tests
- ``src/results.rs`` — result data, CSV/JSON output, share image
- ``src/http.rs`` — HTTP client and request helpers

The original Python implementation has been removed; this repository is the
Rust implementation.

Attribution
-----------

Original ``speedtest-cli`` (Python) by Matt Martz and contributors:
https://github.com/sivel/speedtest-cli

This project is a modified, rewritten work and is distributed under the same
Apache License 2.0. See ``NOTICE``.

Inconsistency
-------------

It is not a goal of this application to be a reliable latency reporting tool.

Latency reported by this tool should not be relied on as a value indicative
of ICMP style latency. It is a relative value used for determining the lowest
latency server for performing the actual speed test against.

Results may differ from Speedtest.net's own client, which uses pure socket
tests rather than HTTP-based tests.

License
-------

Apache License 2.0. See ``LICENSE``.
