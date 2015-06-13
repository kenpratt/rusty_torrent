RustyTorrent
============

A BitTorrent client, written in Rust.

It supports:

* Reading `.torrent` files (single-file torrents only)
* Connecting to a tracker to find peers
* Downloading a file from multiple peers in parallel

Not quite yet:

* Uploading/seeding
* Multi-file torrents
* Queueing multiple requests with each peer for faster downloading
* Connecting to multiple trackers

Requirements
------------

* Rust 1.0.0 or later

Usage
-----

Download and install Rust 1.0 from http://www.rust-lang.org/install.html.

To run:

    cargo run path/to/myfile.torrent

To watch for changes and auto-rebuild (on OS X):

    gem install kicker -s http://gemcutter.org
    ./watch
