* When a handshake response is received, check to ensure the peer_id isn't the same as ours (connecting to ourself), and if so, close that connection.
* Prioritize pieces that are mostly complete, so we can clear them from memory.
* Benchmark CPU usage to try to figure out why we use ~100% while writing files.
* Implement "rarest-first" strategy where peers will prioritize files that they have that not many other peers do.
* Support uploading/seed.
* Support torrents with multiple files.
* Support torrents with multiple trackers.
* How are we ging to close connections
