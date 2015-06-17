* When a handshake response is received, check to ensure the peer_id isn't the same as ours (connecting to ourself), and if so, close that connection.
* Prioritize pieces that are mostly complete, so we can clear them from memory.
* Benchmark CPU usage to try to figure out why we use ~100% while writing files.
* Implement "rarest-first" strategy where peers will prioritize files that they have that not many other peers do.
* Support uploading/seed.
* Support torrents with multiple files.
* Support torrents with multiple trackers.
* How are we ging to close connections
* Announce each X minutes to Tracker that you have a file
* Un-register peers from Download when they close.
* Instead of closing peer when Download completes, close it when neither peer is interested anymore?
* Refactor TcpStream reads to be buffered in a more sensible way, return disconnects as control signal or specialized error.
