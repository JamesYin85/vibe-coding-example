use crate::agent::state::{AgentState, StateMachine};
use crate::capability::CapabilityRegistry;
use crate::communication::{Channel, Message, ResponsePayload};
use crate::error::{AgentError, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use tracing::{debug, info, trace, warn, instrument};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub description: String,
    pub context: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubTask {
    pub id: String,
    pub parent_id: String,
    pub description: String,
    pub parameters: Value,
    pub assigned_to: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Output {
    pub task_id: String,
    pub result: Value,
    pub success: bool,
    pub message: Option<String>,
}

#[async_trait]
pub trait Agent: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;

    async fn understand(&mut self, input: &str) -> Result<Task>;
    async fn decompose(&mut self, task: Task) -> Result<Vec<SubTask>>;
    async fn execute(&mut self, subtask: SubTask) -> Result<Output>;
    async fn communicate(&mut self, message: Message) -> Result<()>;

    fn state(&self) -> &AgentState;
    fn set_state(&mut self, state: AgentState);

    fn capabilities(&self) -> &CapabilityRegistry;
    fn register_capability(&mut self, capability: Arc<dyn crate::capability::Capability>);

    async fn on_message(&mut self, message: Message) -> Result<()> {
        self.communicate(message).await
    }
}

pub struct BaseAgent {
    id: String,
    name: String,
    state_machine: StateMachine,
    capabilities: CapabilityRegistry,
    channel: Option<Channel>,
}

impl BaseAgent {
    #[instrument(skip_all)]
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        let id = id.into();
        let name = name.into();
        debug!(agent_id = %id, name = %name, "Creating new agent");

        Self {
            id: id.clone(),
            name,
            state_machine: StateMachine::new(),
            capabilities: CapabilityRegistry::new(),
            channel: None,
        }
    }

    pub fn with_channel(mut self, channel: Channel) -> Self {
        debug!(agent_id = %self.id, "Attaching channel to agent");
        self.channel = Some(channel);
        self
    }

    pub fn channel(&self) -> Option<&Channel> {
        self.channel.as_ref()
    }

    pub fn transition_state(&mut self, next: AgentState) -> Result<()> {
        debug!(agent_id = %self.id, from = ?self.state(), to = ?next, "Transitioning state");
        self.state_machine.transition(next)
    }

    pub fn can_accept_task(&self) -> bool {
        self.state_machine.can_accept_task()
    }

    pub fn error_message(&self) -> Option<&str> {
        self.state_machine.error_message()
    }

    #[instrument(skip(self, input), fields(agent_id = %self.id, input_len = input.len()))]
    pub async fn process_task(&mut self, input: &str) -> Result<Vec<Output>> {
        info!(agent_id = %self.id, input = %input, "Processing task");

        if !self.can_accept_task() {
            warn!(
                agent_id = %self.id,
                state = ?self.state(),
                "Cannot accept task: invalid state"
            );
            return Err(AgentError::internal(format!(
                "Agent {} cannot accept tasks in state {:?}",
                self.id,
                self.state()
            )));
        }

        self.transition_state(AgentState::Running)?;

        let task = self.understand(input).await.map_err(|e| {
            warn!(agent_id = %self.id, error = %e, "Task understanding failed");
            self.state_machine.fail(e.to_string());
            e
        })?;

        debug!(agent_id = %self.id, task_id = %task.id, "Task understood");

        let subtasks = self.decompose(task.clone()).await.map_err(|e| {
            warn!(agent_id = %self.id, task_id = %task.id, error = %e, "Task decomposition failed");
            self.state_machine.fail(e.to_string());
            e
        })?;

        info!(
            agent_id = %self.id,
            task_id = %task.id,
            subtask_count = subtasks.len(),
            "Task decomposed into subtasks"
        );

        let mut outputs = Vec::new();
        for (idx, subtask) in subtasks.into_iter().enumerate() {
            trace!(
                agent_id = %self.id,
                subtask_id = %subtask.id,
                subtask_idx = idx,
                "Executing subtask"
            );

            match self.execute(subtask).await {
                Ok(output) => {
                    debug!(
                        agent_id = %self.id,
                        subtask_id = %output.task_id,
                        success = output.success,
                        "Subtask completed"
                    );
                    outputs.push(output);
                }
                Err(e) => {
                    warn!(
                        agent_id = %self.id,
                        error = %e,
                        error_category = ?e.category(),
                        "Subtask execution failed"
                    );
                    e.log();
                    self.state_machine.fail(e.to_string());
                    return Err(e);
                }
            }
        }

        self.transition_state(AgentState::Completed)?;
        info!(agent_id = %self.id, outputs_count = outputs.len(), "Task processing completed");

        Ok(outputs)
    }

    pub async fn send_message(&self, _to: &str, message: Message) -> Result<()> {
        trace!(agent_id = %self.id, message_id = %message.id(), "Sending message");
        if let Some(channel) = &self.channel {
            channel.send(message).await
        } else {
            Err(AgentError::communication_error(
                &self.id,
                "No channel configured",
            ))
        }
    }

    pub async fn respond_to_user(&mut self, request_id: &str, result: Value, success: bool) -> Result<()> {
        debug!(
            agent_id = %self.id,
            request_id = %request_id,
            success = success,
            "Responding to user"
        );

        let response = Message::response(
            &self.id,
            ResponsePayload {
                request_id: request_id.to_string(),
                result,
                success,
                error: None,
            },
        );
        self.communicate(response).await
    }

    pub fn reset(&mut self) {
        info!(agent_id = %self.id, "Resetting agent");
        self.state_machine.reset();
    }
}

#[async_trait]
impl Agent for BaseAgent {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    #[instrument(skip(self, input), fields(agent_id = %self.id))]
    async fn understand(&mut self, input: &str) -> Result<Task> {
        debug!(input = %input, "Understanding task");
        Ok(Task {
            id: format!("task_{}", uuid::Uuid::new_v4()),
            description: input.to_string(),
            context: Value::Null,
        })
    }

    #[instrument(skip(self, task), fields(agent_id = %self.id, task_id = %task.id))]
    async fn decompose(&mut self, task: Task) -> Result<Vec<SubTask>> {
        debug!(task_description = %task.description, "Decomposing task");
        Ok(vec![SubTask {
            id: format!("sub_{}", task.id),
            parent_id: task.id,
            description: task.description,
            parameters: Value::Null,
            assigned_to: None,
        }])
    }

    #[instrument(skip(self, subtask), fields(agent_id = %self.id, subtask_id = %subtask.id))]
    async fn execute(&mut self, subtask: SubTask) -> Result<Output> {
        debug!(subtask_description = %subtask.description, "Executing subtask");
        Ok(Output {
            task_id: subtask.id,
            result: Value::Null,
            success: true,
            message: Some("Executed successfully".to_string()),
        })
    }

    #[instrument(skip(self, message), fields(agent_id = %self.id, message_id = %message.id()))]
    async fn communicate(&mut self, message: Message) -> Result<()> {
        trace!(message_type = ?std::mem::discriminant(&message), "Communicating");
        Ok(())
    }

    fn state(&self) -> &AgentState {
        self.state_machine.current()
    }

    fn set_state(&mut self, state: AgentState) {
        self.state_machine.force_set(state);
    }

    fn capabilities(&self) -> &CapabilityRegistry {
        &self.capabilities
    }

    fn register_capability(&mut self, capability: Arc<dyn crate::capability::Capability>) {
        info!(
            agent_id = %self.id,
            capability = %capability.name(),
            "Registering capability"
        );
        self.capabilities.register(capability);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_agent_creation() {
        let agent = BaseAgent::new("agent-1", "Test Agent");
        assert_eq!(agent.id(), "agent-1");
        assert_eq!(agent.name(), "Test Agent");
        assert_eq!(agent.state(), &AgentState::Idle);
    }

    #[tokio::test]
    async fn test_state_transitions() {
        let mut agent = BaseAgent::new("agent-1", "Test Agent");
        agent.transition_state(AgentState::Running).unwrap();
        assert_eq!(agent.state(), &AgentState::Running);
    }

    #[tokio::test]
    async fn test_process_task() {
        let mut agent = BaseAgent::new("agent-1", "Test Agent");
        let outputs = agent.process_task("Do something").await.unwrap();
        assert!(!outputs.is_empty());
        assert_eq!(agent.state(), &AgentState::Completed);
    }

    #[tokio::test]
    async fn test_agent_reset() {
        let mut agent = BaseAgent::new("agent-1", "Test Agent");
        agent.transition_state(AgentState::Running).unwrap();
        agent.state_machine.fail("Test error");
        assert_eq!(agent.state(), &AgentState::Failed);

        agent.reset();
        assert_eq!(agent.state(), &AgentState::Idle);
    }

    #[tokio::test]
    async fn test_error_message() {
        let mut agent = BaseAgent::new("agent-1", "Test Agent");
        agent.transition_state(AgentState::Running).unwrap();
        agent.state_machine.fail("Something went wrong");

        assert_eq!(agent.error_message(), Some("Something went wrong"));
    }
}
