use peer_connection::Message;

#[derive(Clone)]
pub enum IPC {
    BlockComplete(u32, u32),
    DownloadComplete,
    Message(Message),
}
