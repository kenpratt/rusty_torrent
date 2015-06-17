* Benchmark CPU usage to try to figure out why we use ~100% while writing files.
* Implement "rarest-first" strategy where peers will prioritize files that they have that not many other peers do.
* Support torrents with multiple files.
* Support torrents with multiple trackers.
* Announce each X minutes to Tracker that you have a file.
* Announce to tracker when file completes.
* Instead of closing peer when Download completes, close it when neither peer is interested anymore?
* Refactor TcpStream reads to be buffered in a more sensible way, return disconnects as control signal or specialized error.
* Create separate thread for sending messages on TcpStream, so that chunks being uploaded don't block the main control thread for each PeerConnection.
* Only verify the file if it already existed on boot.
