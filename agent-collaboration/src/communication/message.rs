use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};

pub type MessageId = String;

fn generate_id() -> MessageId {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("msg_{}", timestamp)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskPayload {
    pub task_id: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryPayload {
    pub query: String,
    pub context: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponsePayload {
    pub request_id: String,
    pub result: Value,
    pub success: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventPayload {
    pub event_type: String,
    pub data: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    Task {
        id: MessageId,
        from: String,
        to: String,
        payload: TaskPayload,
    },
    Query {
        id: MessageId,
        from: String,
        to: String,
        payload: QueryPayload,
    },
    Response {
        id: MessageId,
        to: String,
        payload: ResponsePayload,
    },
    Event {
        id: MessageId,
        from: String,
        payload: EventPayload,
    },
}

impl Message {
    pub fn task(from: impl Into<String>, to: impl Into<String>, payload: TaskPayload) -> Self {
        Self::Task {
            id: generate_id(),
            from: from.into(),
            to: to.into(),
            payload,
        }
    }

    pub fn query(from: impl Into<String>, to: impl Into<String>, payload: QueryPayload) -> Self {
        Self::Query {
            id: generate_id(),
            from: from.into(),
            to: to.into(),
            payload,
        }
    }

    pub fn response(to: impl Into<String>, payload: ResponsePayload) -> Self {
        Self::Response {
            id: generate_id(),
            to: to.into(),
            payload,
        }
    }

    pub fn event(from: impl Into<String>, payload: EventPayload) -> Self {
        Self::Event {
            id: generate_id(),
            from: from.into(),
            payload,
        }
    }

    pub fn id(&self) -> &str {
        match self {
            Message::Task { id, .. } => id,
            Message::Query { id, .. } => id,
            Message::Response { id, .. } => id,
            Message::Event { id, .. } => id,
        }
    }

    pub fn is_for(&self, agent_id: &str) -> bool {
        match self {
            Message::Task { to, .. } | Message::Query { to, .. } | Message::Response { to, .. } => {
                to == agent_id
            }
            Message::Event { .. } => true, // Events are broadcast
        }
    }
}
