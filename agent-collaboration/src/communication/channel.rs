use crate::communication::Message;
use crate::error::{AgentError, Result};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, Mutex};
use tracing::{debug, trace, warn, instrument};

pub type Sender = mpsc::Sender<Message>;
pub type Receiver = mpsc::Receiver<Message>;

pub struct Channel {
    id: String,
    sender: Sender,
    receiver: Arc<Mutex<Receiver>>,
    broadcast_tx: broadcast::Sender<Message>,
    broadcast_rx: Arc<Mutex<broadcast::Receiver<Message>>>,
}

impl Channel {
    #[instrument(skip_all)]
    pub fn new(id: impl Into<String>, buffer_size: usize) -> Self {
        let id = id.into();
        debug!(channel_id = %id, buffer_size, "Creating new channel");

        let (sender, receiver) = mpsc::channel(buffer_size);
        let (broadcast_tx, broadcast_rx) = broadcast::channel(buffer_size);

        Self {
            id,
            sender,
            receiver: Arc::new(Mutex::new(receiver)),
            broadcast_tx,
            broadcast_rx: Arc::new(Mutex::new(broadcast_rx)),
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn sender(&self) -> Sender {
        self.sender.clone()
    }

    pub fn broadcast_sender(&self) -> broadcast::Sender<Message> {
        self.broadcast_tx.clone()
    }

    #[instrument(skip(self, message), fields(channel_id = %self.id, message_id = %message.id()))]
    pub async fn send(&self, message: Message) -> Result<()> {
        trace!(channel_id = %self.id, message_id = %message.id(), "Sending message");

        self.sender
            .send(message)
            .await
            .map_err(|e| {
                warn!(channel_id = %self.id, error = %e, "Failed to send message");
                AgentError::channel_error(e.to_string())
            })
    }

    #[instrument(skip(self), fields(channel_id = %self.id))]
    pub async fn recv(&self) -> Option<Message> {
        trace!(channel_id = %self.id, "Waiting for message");

        let msg = self.receiver.lock().await.recv().await;

        if let Some(ref m) = msg {
            trace!(channel_id = %self.id, message_id = %m.id(), "Received message");
        }

        msg
    }

    #[instrument(skip(self, message), fields(channel_id = %self.id, message_id = %message.id()))]
    pub async fn broadcast(&self, message: Message) -> Result<()> {
        trace!(channel_id = %self.id, message_id = %message.id(), "Broadcasting message");

        self.broadcast_tx
            .send(message)
            .map_err(|e| {
                warn!(channel_id = %self.id, error = %e, "Failed to broadcast message");
                AgentError::channel_error(e.to_string())
            })?;

        Ok(())
    }

    #[instrument(skip(self), fields(channel_id = %self.id))]
    pub async fn recv_broadcast(&self) -> Result<Message> {
        trace!(channel_id = %self.id, "Waiting for broadcast message");

        self.broadcast_rx
            .lock()
            .await
            .recv()
            .await
            .map_err(|e| {
                warn!(channel_id = %self.id, error = %e, "Failed to receive broadcast");
                AgentError::channel_error(e.to_string())
            })
    }
}

pub struct ChannelManager {
    channels: Arc<Mutex<std::collections::HashMap<String, Channel>>>,
}

impl Default for ChannelManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ChannelManager {
    pub fn new() -> Self {
        debug!("Creating new channel manager");
        Self {
            channels: Arc::new(Mutex::new(std::collections::HashMap::new())),
        }
    }

    #[instrument(skip(self), fields(agent_id = %agent_id))]
    pub async fn create_channel(&self, agent_id: &str, buffer_size: usize) -> Channel {
        debug!(agent_id = %agent_id, buffer_size, "Creating channel for agent");

        let channel = Channel::new(agent_id, buffer_size);
        self.channels.lock().await.insert(agent_id.to_string(), Channel::new(agent_id, buffer_size));

        channel
    }

    pub async fn get_channel(&self, agent_id: &str) -> Option<Channel> {
        self.channels.lock().await.get(agent_id).map(|_| Channel::new(agent_id, 16))
    }

    #[instrument(skip(self, message), fields(agent_id = %agent_id, message_id = %message.id()))]
    pub async fn send_to(&self, agent_id: &str, message: Message) -> Result<()> {
        trace!(agent_id = %agent_id, message_id = %message.id(), "Sending message to agent");

        let channels = self.channels.lock().await;
        if let Some(channel) = channels.get(agent_id) {
            channel.send(message).await
        } else {
            warn!(agent_id = %agent_id, "Agent channel not found");
            Err(AgentError::channel_error(format!(
                "Agent {} not found",
                agent_id
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::communication::EventPayload;

    #[tokio::test]
    async fn test_send_receive() {
        let channel = Channel::new("test-agent", 16);

        let msg = Message::event("sender", EventPayload {
            event_type: "test".to_string(),
            data: serde_json::json!({}),
        });

        channel.send(msg.clone()).await.unwrap();
        let received = channel.recv().await.unwrap();
        assert_eq!(received.id(), msg.id());
    }
}
