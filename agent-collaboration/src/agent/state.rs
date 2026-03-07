use crate::error::{AgentError, Result};
use serde::{Deserialize, Serialize};
use std::time::Instant;
use tracing::{debug, info, warn};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AgentState {
    Idle,
    Running,
    Paused,
    Waiting,
    Cancelling,
    Completed,
    Failed,
}

impl Default for AgentState {
    fn default() -> Self {
        Self::Idle
    }
}

impl std::fmt::Display for AgentState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentState::Idle => write!(f, "Idle"),
            AgentState::Running => write!(f, "Running"),
            AgentState::Paused => write!(f, "Paused"),
            AgentState::Waiting => write!(f, "Waiting"),
            AgentState::Cancelling => write!(f, "Cancelling"),
            AgentState::Completed => write!(f, "Completed"),
            AgentState::Failed => write!(f, "Failed"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct StateRecord {
    pub state: AgentState,
    pub timestamp: Instant,
    pub reason: Option<String>,
}

impl StateRecord {
    pub fn new(state: AgentState, reason: Option<String>) -> Self {
        Self {
            state,
            timestamp: Instant::now(),
            reason,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TransitionRule {
    pub from: AgentState,
    pub to: AgentState,
}

impl TransitionRule {
    pub const fn new(from: AgentState, to: AgentState) -> Self {
        Self { from, to }
    }

    pub fn matches(&self, from: AgentState, to: AgentState) -> bool {
        self.from == from && self.to == to
    }
}

pub struct StateMachine {
    current: AgentState,
    error_message: Option<String>,
    history: Vec<StateRecord>,
    rules: Vec<TransitionRule>,
}

impl Default for StateMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl StateMachine {
    pub fn new() -> Self {
        let rules = vec![
            // From Idle
            TransitionRule::new(AgentState::Idle, AgentState::Running),
            TransitionRule::new(AgentState::Idle, AgentState::Waiting),
            // From Running
            TransitionRule::new(AgentState::Running, AgentState::Completed),
            TransitionRule::new(AgentState::Running, AgentState::Failed),
            TransitionRule::new(AgentState::Running, AgentState::Paused),
            TransitionRule::new(AgentState::Running, AgentState::Waiting),
            TransitionRule::new(AgentState::Running, AgentState::Cancelling),
            // From Paused
            TransitionRule::new(AgentState::Paused, AgentState::Running),
            TransitionRule::new(AgentState::Paused, AgentState::Cancelling),
            // From Waiting
            TransitionRule::new(AgentState::Waiting, AgentState::Running),
            TransitionRule::new(AgentState::Waiting, AgentState::Cancelling),
            // From Cancelling
            TransitionRule::new(AgentState::Cancelling, AgentState::Completed),
            TransitionRule::new(AgentState::Cancelling, AgentState::Failed),
            // Terminal states can reset to Idle
            TransitionRule::new(AgentState::Completed, AgentState::Idle),
            TransitionRule::new(AgentState::Failed, AgentState::Idle),
        ];

        let mut sm = Self {
            current: AgentState::Idle,
            error_message: None,
            history: Vec::new(),
            rules,
        };
        sm.record_state(AgentState::Idle, None);
        sm
    }

    fn record_state(&mut self, state: AgentState, reason: Option<String>) {
        self.history.push(StateRecord::new(state, reason));
    }

    pub fn current(&self) -> &AgentState {
        &self.current
    }

    pub fn error_message(&self) -> Option<&str> {
        self.error_message.as_deref()
    }

    pub fn history(&self) -> &[StateRecord] {
        &self.history
    }

    pub fn is_valid_transition(&self, to: AgentState) -> bool {
        self.rules
            .iter()
            .any(|rule| rule.matches(self.current, to))
    }

    pub fn transition(&mut self, next: AgentState) -> Result<()> {
        if self.is_valid_transition(next) {
            debug!(
                from = ?self.current,
                to = ?next,
                "State transition"
            );

            self.current = next;
            self.error_message = None;
            self.record_state(next, None);

            info!(state = ?next, "Agent state changed");
            Ok(())
        } else {
            warn!(
                from = ?self.current,
                to = ?next,
                "Invalid state transition attempted"
            );
            Err(AgentError::InvalidStateTransition {
                from: self.current.to_string(),
                to: next.to_string(),
            })
        }
    }

    pub fn transition_with_reason(&mut self, next: AgentState, reason: impl Into<String>) -> Result<()> {
        let reason_str = reason.into();

        if self.is_valid_transition(next) {
            debug!(
                from = ?self.current,
                to = ?next,
                reason = %reason_str,
                "State transition with reason"
            );

            self.current = next;
            self.record_state(next, Some(reason_str.clone()));

            if next == AgentState::Failed {
                self.error_message = Some(reason_str.clone());
                warn!(reason = %reason_str, "Agent entered Failed state");
            } else {
                info!(state = ?next, reason = %reason_str, "Agent state changed");
            }

            Ok(())
        } else {
            warn!(
                from = ?self.current,
                to = ?next,
                "Invalid state transition attempted"
            );
            Err(AgentError::InvalidStateTransition {
                from: self.current.to_string(),
                to: next.to_string(),
            })
        }
    }

    pub fn fail(&mut self, error: impl Into<String>) {
        let error_str = error.into();
        warn!(error = %error_str, "Agent failing");

        self.current = AgentState::Failed;
        self.error_message = Some(error_str.clone());
        self.record_state(AgentState::Failed, Some(error_str));
    }

    pub fn force_set(&mut self, state: AgentState) {
        debug!(from = ?self.current, to = ?state, "Force setting state");
        self.current = state;
        self.error_message = None;
        self.record_state(state, Some("forced".to_string()));
    }

    pub fn reset(&mut self) {
        debug!("Resetting state machine to Idle");
        self.current = AgentState::Idle;
        self.error_message = None;
        self.record_state(AgentState::Idle, Some("reset".to_string()));
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self.current, AgentState::Completed | AgentState::Failed)
    }

    pub fn is_running(&self) -> bool {
        matches!(self.current, AgentState::Running)
    }

    pub fn is_failed(&self) -> bool {
        matches!(self.current, AgentState::Failed)
    }

    pub fn can_accept_task(&self) -> bool {
        matches!(
            self.current,
            AgentState::Idle | AgentState::Completed | AgentState::Failed
        )
    }

    pub fn last_state_duration(&self) -> Option<std::time::Duration> {
        if self.history.len() >= 2 {
            let current = &self.history[self.history.len() - 1];
            let previous = &self.history[self.history.len() - 2];
            Some(current.timestamp.duration_since(previous.timestamp))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state() {
        let sm = StateMachine::new();
        assert_eq!(sm.current(), &AgentState::Idle);
        assert!(sm.error_message().is_none());
    }

    #[test]
    fn test_valid_transition() {
        let mut sm = StateMachine::new();
        sm.transition(AgentState::Running).unwrap();
        assert_eq!(sm.current(), &AgentState::Running);
    }

    #[test]
    fn test_invalid_transition() {
        let mut sm = StateMachine::new();
        assert!(sm.transition(AgentState::Completed).is_err());
    }

    #[test]
    fn test_full_lifecycle() {
        let mut sm = StateMachine::new();
        sm.transition(AgentState::Running).unwrap();
        sm.transition(AgentState::Completed).unwrap();
        assert!(sm.is_terminal());
    }

    #[test]
    fn test_fail_with_message() {
        let mut sm = StateMachine::new();
        sm.transition(AgentState::Running).unwrap();
        sm.fail("Something went wrong");

        assert_eq!(sm.current(), &AgentState::Failed);
        assert_eq!(sm.error_message(), Some("Something went wrong"));
    }

    #[test]
    fn test_transition_with_reason() {
        let mut sm = StateMachine::new();
        sm.transition(AgentState::Running).unwrap();
        sm.transition_with_reason(AgentState::Paused, "User requested pause").unwrap();

        assert_eq!(sm.current(), &AgentState::Paused);
        assert!(sm.history().len() >= 3);
    }

    #[test]
    fn test_reset() {
        let mut sm = StateMachine::new();
        sm.transition(AgentState::Running).unwrap();
        sm.fail("Error");
        sm.reset();

        assert_eq!(sm.current(), &AgentState::Idle);
        assert!(sm.error_message().is_none());
    }

    #[test]
    fn test_history() {
        let mut sm = StateMachine::new();
        sm.transition(AgentState::Running).unwrap();
        sm.transition(AgentState::Completed).unwrap();

        assert!(sm.history().len() >= 3);
        assert!(sm.last_state_duration().is_some());
    }
}
