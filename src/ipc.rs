use peer_connection::Message;

pub enum IPC {
    CancelRequest(u32, u32),
    Message(Message),
}