* Support torrents with multiple files.
* Queue multiple requests with each peer.
* Find better way to avoid downloading from ourself than hard-coding IP address (probably the easiest way is to try connecting to ourself and than checking the peer_id, as below).
* When a handshake response is received, check to ensure the peer_id isn't the same as ours (connecting to ourself), and if so, close that connection.
* Support torrents with multiple trackers.
