RustyTorrent
============

A BitTorrent client, written in Rust.

It supports:

* Reading `.torrent` files (single-file torrents only)
* Connecting to a tracker to discover peers
* Downloading a file from multiple peers in parallel
* Queueing multiple requests with each peer for faster downloading (aka pipelining)
* Uploading files to peers, and seeding existing files from disk
* Resuming partial downloads
* Verification of correctness of downloaded chunks

Not yet:

* Multi-file torrents
* Connecting to multiple trackers
* Upload throttling/congestion control
* NAT traversal

Requirements
------------

* Rust 1.0.0 or later

Usage
-----

Download and install Rust 1.0 from http://www.rust-lang.org/install.html.

Clone the repository:

    git clone git@github.com:kenpratt/rusty_torrent.git
    cd rusty_torrent

To run:

    cargo run path/to/myfile.torrent

To run specifying a port to listen on:

    cargo run -- -p 3333 path/to/myfile.torrent

Your file will be saved in the `downloads/` directory.

To watch for changes and auto-rebuild (on OS X):

    gem install kicker -s http://gemcutter.org
    ./watch
