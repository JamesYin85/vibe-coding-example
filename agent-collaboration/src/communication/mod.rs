mod channel;
mod message;

pub use channel::{Channel, ChannelManager, Receiver, Sender};
pub use message::{EventPayload, Message, MessageId, QueryPayload, ResponsePayload, TaskPayload};
