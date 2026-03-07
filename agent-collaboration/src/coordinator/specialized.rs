//! Specialized Agents for Code Analysis
//!
//! Provides specialized agents for different aspects of code analysis.

use crate::agent::{Agent, BaseAgent, Output, SubTask, Task, AgentState};
use crate::capability::CapabilityRegistry;
use crate::error::Result;
use crate::communication::Message;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::{info, instrument};

/// Specialized Agent Trait - extends Agent with domain-specific capabilities
#[async_trait]
pub trait SpecializedAgent: Agent {
    /// Returns the agent's domain of expertise
    fn domain(&self) -> &str;

    /// Returns the agent's expertise keywords
    fn expertise(&self) -> Vec<&str>;

    /// Check if this agent can handle a specific task type
    fn can_handle(&self, task_type: &str) -> bool {
        self.expertise().iter().any(|e| task_type.to_lowercase().contains(&e.to_lowercase()))
    }
}

/// Code Style Analysis Agent
pub struct CodeStyleAgent {
    base: BaseAgent,
}

impl CodeStyleAgent {
    pub fn new(id: &str) -> Self {
        Self {
            base: BaseAgent::new(id, "Code Style Analyzer"),
        }
    }

    fn analyze_style(&self, code: &str) -> Value {
        let mut issues = Vec::new();
        let mut score = 100u32;

        // Check line length
        for (line_num, line) in code.lines().enumerate() {
            if line.len() > 100 {
                issues.push(json!({
                    "type": "line_too_long",
                    "line": line_num + 1,
                    "length": line.len(),
                    "suggestion": "Consider breaking long lines for better readability"
                }));
                score = score.saturating_sub(2);
            }
        }

        // Check naming conventions (simplified)
        if code.contains("_") && code.contains("camelCase") {
            issues.push(json!({
                "type": "inconsistent_naming",
                "suggestion": "Use consistent naming convention (either snake_case or camelCase)"
            }));
            score = score.saturating_sub(5);
        }

        // Check indentation consistency
        let has_tabs = code.contains('\t');
        let has_spaces = code.contains("    ");
        if has_tabs && has_spaces {
            issues.push(json!({
                "type": "mixed_indentation",
                "suggestion": "Use consistent indentation (tabs or spaces, not both)"
            }));
            score = score.saturating_sub(10);
        }

        // Check for TODO/FIXME comments
        let todo_count = code.matches("TODO").count();
        let fixme_count = code.matches("FIXME").count();
        if todo_count > 0 || fixme_count > 0 {
            issues.push(json!({
                "type": "pending_todos",
                "todo_count": todo_count,
                "fixme_count": fixme_count,
                "suggestion": "Consider resolving pending TODO/FIXME items"
            }));
            score = score.saturating_sub(3);
        }

        json!({
            "domain": "style",
            "score": score,
            "issues": issues,
            "summary": format!("Style score: {}/100", score)
        })
    }
}

#[async_trait]
impl Agent for CodeStyleAgent {
    fn id(&self) -> &str {
        self.base.id()
    }

    fn name(&self) -> &str {
        self.base.name()
    }

    async fn understand(&mut self, input: &str) -> Result<Task> {
        self.base.understand(input).await
    }

    async fn decompose(&mut self, task: Task) -> Result<Vec<SubTask>> {
        self.base.decompose(task).await
    }

    #[instrument(skip(self, subtask))]
    async fn execute(&mut self, subtask: SubTask) -> Result<Output> {
        info!(agent = %self.name(), subtask_id = %subtask.id, "Executing style analysis");

        let code = subtask.parameters.get("code")
            .and_then(|v| v.as_str())
            .unwrap_or(&subtask.description);

        let result = self.analyze_style(code);

        Ok(Output {
            task_id: subtask.id,
            result,
            success: true,
            message: Some("Code style analysis completed".to_string()),
        })
    }

    async fn communicate(&mut self, message: Message) -> Result<()> {
        self.base.communicate(message).await
    }

    fn state(&self) -> &AgentState {
        self.base.state()
    }

    fn set_state(&mut self, state: AgentState) {
        self.base.set_state(state);
    }

    fn capabilities(&self) -> &CapabilityRegistry {
        self.base.capabilities()
    }

    fn register_capability(&mut self, capability: Arc<dyn crate::capability::Capability>) {
        self.base.register_capability(capability);
    }
}

#[async_trait]
impl SpecializedAgent for CodeStyleAgent {
    fn domain(&self) -> &str {
        "code_style"
    }

    fn expertise(&self) -> Vec<&str> {
        vec!["style", "formatting", "naming", "readability", "conventions"]
    }
}

/// Code Security Analysis Agent
pub struct CodeSecurityAgent {
    base: BaseAgent,
}

impl CodeSecurityAgent {
    pub fn new(id: &str) -> Self {
        Self {
            base: BaseAgent::new(id, "Code Security Analyzer"),
        }
    }

    fn analyze_security(&self, code: &str) -> Value {
        let mut vulnerabilities = Vec::new();
        let mut score = 100u32;

        // Check for potential SQL injection
        if code.contains("format!") && code.contains("SELECT") {
            vulnerabilities.push(json!({
                "type": "potential_sql_injection",
                "severity": "high",
                "suggestion": "Use parameterized queries instead of string formatting"
            }));
            score = score.saturating_sub(20);
        }

        // Check for hardcoded secrets
        let secret_patterns = ["password", "api_key", "secret", "token"];
        for pattern in secret_patterns {
            if code.to_lowercase().contains(&format!("{} =", pattern)) ||
               code.to_lowercase().contains(&format!("{}=\"", pattern)) {
                vulnerabilities.push(json!({
                    "type": "hardcoded_secret",
                    "severity": "critical",
                    "pattern": pattern,
                    "suggestion": "Move secrets to environment variables or secure storage"
                }));
                score = score.saturating_sub(30);
            }
        }

        // Check for unsafe operations
        if code.contains("unsafe") {
            vulnerabilities.push(json!({
                "type": "unsafe_block",
                "severity": "medium",
                "suggestion": "Review unsafe blocks for potential memory safety issues"
            }));
            score = score.saturating_sub(10);
        }

        // Check for unwrap() usage
        let unwrap_count = code.matches(".unwrap()").count();
        if unwrap_count > 0 {
            vulnerabilities.push(json!({
                "type": "unwrap_usage",
                "severity": "low",
                "count": unwrap_count,
                "suggestion": "Consider using proper error handling instead of unwrap()"
            }));
            score = score.saturating_sub(5 * unwrap_count.min(3) as u32);
        }

        json!({
            "domain": "security",
            "score": score,
            "vulnerabilities": vulnerabilities,
            "summary": format!("Security score: {}/100", score)
        })
    }
}

#[async_trait]
impl Agent for CodeSecurityAgent {
    fn id(&self) -> &str {
        self.base.id()
    }

    fn name(&self) -> &str {
        self.base.name()
    }

    async fn understand(&mut self, input: &str) -> Result<Task> {
        self.base.understand(input).await
    }

    async fn decompose(&mut self, task: Task) -> Result<Vec<SubTask>> {
        self.base.decompose(task).await
    }

    #[instrument(skip(self, subtask))]
    async fn execute(&mut self, subtask: SubTask) -> Result<Output> {
        info!(agent = %self.name(), subtask_id = %subtask.id, "Executing security analysis");

        let code = subtask.parameters.get("code")
            .and_then(|v| v.as_str())
            .unwrap_or(&subtask.description);

        let result = self.analyze_security(code);

        Ok(Output {
            task_id: subtask.id,
            result,
            success: true,
            message: Some("Code security analysis completed".to_string()),
        })
    }

    async fn communicate(&mut self, message: Message) -> Result<()> {
        self.base.communicate(message).await
    }

    fn state(&self) -> &AgentState {
        self.base.state()
    }

    fn set_state(&mut self, state: AgentState) {
        self.base.set_state(state);
    }

    fn capabilities(&self) -> &CapabilityRegistry {
        self.base.capabilities()
    }

    fn register_capability(&mut self, capability: Arc<dyn crate::capability::Capability>) {
        self.base.register_capability(capability);
    }
}

#[async_trait]
impl SpecializedAgent for CodeSecurityAgent {
    fn domain(&self) -> &str {
        "code_security"
    }

    fn expertise(&self) -> Vec<&str> {
        vec!["security", "vulnerability", "injection", "authentication", "secrets"]
    }
}

/// Code Performance Analysis Agent
pub struct CodePerformanceAgent {
    base: BaseAgent,
}

impl CodePerformanceAgent {
    pub fn new(id: &str) -> Self {
        Self {
            base: BaseAgent::new(id, "Code Performance Analyzer"),
        }
    }

    fn analyze_performance(&self, code: &str) -> Value {
        let mut issues = Vec::new();
        let mut score = 100u32;

        // Check for nested loops (O(n^2) complexity)
        let nested_loop_count = code.matches("for").count() / 2;
        if nested_loop_count > 0 {
            issues.push(json!({
                "type": "nested_loops",
                "severity": "medium",
                "suggestion": "Consider optimizing nested loops or using more efficient algorithms"
            }));
            score = score.saturating_sub(10 * nested_loop_count.min(3) as u32);
        }

        // Check for clone() in loops
        if code.contains("for") && code.contains(".clone()") {
            issues.push(json!({
                "type": "clone_in_loop",
                "severity": "medium",
                "suggestion": "Consider using references instead of cloning inside loops"
            }));
            score = score.saturating_sub(15);
        }

        // Check for unnecessary allocations
        if code.contains("String::from(") && code.contains("&str") {
            issues.push(json!({
                "type": "unnecessary_allocation",
                "severity": "low",
                "suggestion": "Consider using &str where possible to avoid allocations"
            }));
            score = score.saturating_sub(5);
        }

        // Check for async in loop
        if code.contains("for") && code.contains(".await") {
            issues.push(json!({
                "type": "sequential_async",
                "severity": "high",
                "suggestion": "Consider using futures::join! or concurrent execution"
            }));
            score = score.saturating_sub(20);
        }

        json!({
            "domain": "performance",
            "score": score,
            "issues": issues,
            "summary": format!("Performance score: {}/100", score)
        })
    }
}

#[async_trait]
impl Agent for CodePerformanceAgent {
    fn id(&self) -> &str {
        self.base.id()
    }

    fn name(&self) -> &str {
        self.base.name()
    }

    async fn understand(&mut self, input: &str) -> Result<Task> {
        self.base.understand(input).await
    }

    async fn decompose(&mut self, task: Task) -> Result<Vec<SubTask>> {
        self.base.decompose(task).await
    }

    #[instrument(skip(self, subtask))]
    async fn execute(&mut self, subtask: SubTask) -> Result<Output> {
        info!(agent = %self.name(), subtask_id = %subtask.id, "Executing performance analysis");

        let code = subtask.parameters.get("code")
            .and_then(|v| v.as_str())
            .unwrap_or(&subtask.description);

        let result = self.analyze_performance(code);

        Ok(Output {
            task_id: subtask.id,
            result,
            success: true,
            message: Some("Code performance analysis completed".to_string()),
        })
    }

    async fn communicate(&mut self, message: Message) -> Result<()> {
        self.base.communicate(message).await
    }

    fn state(&self) -> &AgentState {
        self.base.state()
    }

    fn set_state(&mut self, state: AgentState) {
        self.base.set_state(state);
    }

    fn capabilities(&self) -> &CapabilityRegistry {
        self.base.capabilities()
    }

    fn register_capability(&mut self, capability: Arc<dyn crate::capability::Capability>) {
        self.base.register_capability(capability);
    }
}

#[async_trait]
impl SpecializedAgent for CodePerformanceAgent {
    fn domain(&self) -> &str {
        "code_performance"
    }

    fn expertise(&self) -> Vec<&str> {
        vec!["performance", "optimization", "speed", "memory", "efficiency"]
    }
}

/// Code Structure Analysis Agent
pub struct CodeStructureAgent {
    base: BaseAgent,
}

impl CodeStructureAgent {
    pub fn new(id: &str) -> Self {
        Self {
            base: BaseAgent::new(id, "Code Structure Analyzer"),
        }
    }

    fn analyze_structure(&self, code: &str) -> Value {
        let mut metrics = serde_json::Map::new();
        let mut score = 100u32;

        // Count functions
        let fn_count = code.matches("fn ").count();
        metrics.insert("function_count".to_string(), json!(fn_count));

        // Count structs
        let struct_count = code.matches("struct ").count();
        metrics.insert("struct_count".to_string(), json!(struct_count));

        // Count impls
        let impl_count = code.matches("impl ").count();
        metrics.insert("impl_count".to_string(), json!(impl_count));

        // Count traits
        let trait_count = code.matches("trait ").count();
        metrics.insert("trait_count".to_string(), json!(trait_count));

        // Calculate lines of code
        let loc = code.lines().count();
        metrics.insert("lines_of_code".to_string(), json!(loc));

        // Check for documentation
        let doc_comments = code.matches("///").count();
        let has_docs = doc_comments > 0;
        metrics.insert("has_documentation".to_string(), json!(has_docs));
        metrics.insert("doc_comment_count".to_string(), json!(doc_comments));

        if !has_docs && fn_count > 3 {
            score = score.saturating_sub(10);
        }

        // Check for tests
        let has_tests = code.contains("#[test]");
        metrics.insert("has_tests".to_string(), json!(has_tests));
        if !has_tests && fn_count > 3 {
            score = score.saturating_sub(10);
        }

        // Calculate average function length (simplified)
        let avg_fn_length = if fn_count > 0 { loc / fn_count } else { 0 };
        metrics.insert("avg_function_length".to_string(), json!(avg_fn_length));

        if avg_fn_length > 50 {
            score = score.saturating_sub(15);
        }

        json!({
            "domain": "structure",
            "score": score,
            "metrics": metrics,
            "summary": format!(
                "Structure: {} functions, {} structs, {} LOC, docs: {}, tests: {}",
                fn_count, struct_count, loc, has_docs, has_tests
            )
        })
    }
}

#[async_trait]
impl Agent for CodeStructureAgent {
    fn id(&self) -> &str {
        self.base.id()
    }

    fn name(&self) -> &str {
        self.base.name()
    }

    async fn understand(&mut self, input: &str) -> Result<Task> {
        self.base.understand(input).await
    }

    async fn decompose(&mut self, task: Task) -> Result<Vec<SubTask>> {
        self.base.decompose(task).await
    }

    #[instrument(skip(self, subtask))]
    async fn execute(&mut self, subtask: SubTask) -> Result<Output> {
        info!(agent = %self.name(), subtask_id = %subtask.id, "Executing structure analysis");

        let code = subtask.parameters.get("code")
            .and_then(|v| v.as_str())
            .unwrap_or(&subtask.description);

        let result = self.analyze_structure(code);

        Ok(Output {
            task_id: subtask.id,
            result,
            success: true,
            message: Some("Code structure analysis completed".to_string()),
        })
    }

    async fn communicate(&mut self, message: Message) -> Result<()> {
        self.base.communicate(message).await
    }

    fn state(&self) -> &AgentState {
        self.base.state()
    }

    fn set_state(&mut self, state: AgentState) {
        self.base.set_state(state);
    }

    fn capabilities(&self) -> &CapabilityRegistry {
        self.base.capabilities()
    }

    fn register_capability(&mut self, capability: Arc<dyn crate::capability::Capability>) {
        self.base.register_capability(capability);
    }
}

#[async_trait]
impl SpecializedAgent for CodeStructureAgent {
    fn domain(&self) -> &str {
        "code_structure"
    }

    fn expertise(&self) -> Vec<&str> {
        vec!["structure", "architecture", "organization", "design", "metrics"]
    }
}

/// Generic Code Analysis Agent that combines all analysis types
pub struct CodeAnalysisAgent {
    base: BaseAgent,
}

impl CodeAnalysisAgent {
    pub fn new(id: &str) -> Self {
        Self {
            base: BaseAgent::new(id, "Code Quality Analyzer"),
        }
    }
}

#[async_trait]
impl Agent for CodeAnalysisAgent {
    fn id(&self) -> &str {
        self.base.id()
    }

    fn name(&self) -> &str {
        self.base.name()
    }

    async fn understand(&mut self, input: &str) -> Result<Task> {
        self.base.understand(input).await
    }

    async fn decompose(&mut self, task: Task) -> Result<Vec<SubTask>> {
        self.base.decompose(task).await
    }

    async fn execute(&mut self, subtask: SubTask) -> Result<Output> {
        self.base.execute(subtask).await
    }

    async fn communicate(&mut self, message: Message) -> Result<()> {
        self.base.communicate(message).await
    }

    fn state(&self) -> &AgentState {
        self.base.state()
    }

    fn set_state(&mut self, state: AgentState) {
        self.base.set_state(state);
    }

    fn capabilities(&self) -> &CapabilityRegistry {
        self.base.capabilities()
    }

    fn register_capability(&mut self, capability: Arc<dyn crate::capability::Capability>) {
        self.base.register_capability(capability);
    }
}

#[async_trait]
impl SpecializedAgent for CodeAnalysisAgent {
    fn domain(&self) -> &str {
        "code_analysis"
    }

    fn expertise(&self) -> Vec<&str> {
        vec!["quality", "analysis", "review", "assessment"]
    }
}
