//! Code Analysis Demo - Demonstrates the multi-agent coordination system
//!
//! Run with: cargo run --example code_analysis_demo

use agent_collaboration::{
    Coordinator, init_logging_with_config, LoggingConfig,
};
use tracing::info;

/// Sample code to analyze
const SAMPLE_CODE: &str = r#"
use std::collections::HashMap;

// TODO: Add proper error handling
pub fn process_data(input: &str) -> Result<String, String> {
    let api_key = "sk-secret-key-12345";  // Hardcoded secret!

    let data = parse_input(input)?;

    let mut results = HashMap::new();
    for item in &data {
        for sub_item in &item.children {  // Nested loops - O(n^2)
            let key = format!("{}_{}", item.id, sub_item.id);
            results.insert(key, sub_item.value.clone());
        }
    }

    // SQL injection vulnerability
    let query = format!("SELECT * FROM users WHERE id = '{}'", input);

    Ok(serde_json::to_string(&results).unwrap())
}

fn parse_input(input: &str) -> Result<Vec<DataItem>, String> {
    // TODO: Implement proper parsing
    let items: Vec<DataItem> = input.parse().map_err(|e| format!("Parse error: {}", e))?;
    Ok(items)
}

#[derive(Debug, Clone)]
struct DataItem {
    id: String,
    children: Vec<SubItem>,
    value: String,
}

#[derive(Debug, Clone)]
struct SubItem {
    id: String,
    value: String,
}
"#;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    init_logging_with_config(LoggingConfig {
        env_filter: "debug".to_string(),
        with_ansi: true,
        with_target: true,
        with_thread_ids: false,
    });

    println!("╔════════════════════════════════════════════════════════════╗");
    println!("║          Multi-Agent Code Analysis Demo                    ║");
    println!("╚════════════════════════════════════════════════════════════╝");
    println!();

    // Create coordinator
    info!("Creating coordinator...");
    let mut coordinator = Coordinator::new("main-coordinator");

    println!("📋 Coordinator initialized with specialized agents:");
    println!("   • Style Agent - Analyzes code style and formatting");
    println!("   • Security Agent - Detects security vulnerabilities");
    println!("   • Performance Agent - Identifies performance issues");
    println!("   • Structure Agent - Evaluates code organization");
    println!("   • General Analyzer - Overall quality assessment");
    println!();

    println!("📝 Input Task: \"帮我分析这段代码的质量\"");
    println!();

    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("                    📊 CODE TO ANALYZE                        ");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("{}", SAMPLE_CODE.lines().take(15).collect::<Vec<_>>().join("\n"));
    println!("... ({} lines total)", SAMPLE_CODE.lines().count());
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!();

    // Step 1: Analyze and decompose task
    println!("🔄 Step 1: Task Analysis & Decomposition");
    println!("─────────────────────────────────────────");

    // Process the code
    let result = coordinator.analyze_code(SAMPLE_CODE).await?;

    // Show task analysis
    println!("   Task ID: {}", result.task_id);
    println!("   Complexity: {:?}", result.analysis.complexity);
    println!("   Estimated steps: {}", result.analysis.estimated_steps);
    println!("   Dependencies: {}", result.analysis.dependencies.len());
    println!();

    // Show decomposition
    println!("🔄 Step 2: Subtask Assignment");
    println!("─────────────────────────────");
    println!("   Total subtasks: {}", result.subtask_count);
    println!();

    // Show execution results
    println!("🔄 Step 3: Parallel Execution Results");
    println!("──────────────────────────────────────");

    for (agent_id, output) in &result.agent_outputs {
        println!();
        println!("   ╭─ Agent: {} ─────────────────────────", agent_id);

        if let Some(domain) = output.result.get("domain").and_then(|d| d.as_str()) {
            println!("   │ Domain: {}", domain);
        }

        if let Some(score) = output.result.get("score").and_then(|s| s.as_u64()) {
            let bar = "█".repeat((score / 10) as usize);
            let empty = "░".repeat(10 - (score / 10) as usize);
            println!("   │ Score:  [{}{}] {}/100", bar, empty, score);
        }

        if let Some(summary) = output.result.get("summary").and_then(|s| s.as_str()) {
            println!("   │ Summary: {}", summary);
        }

        // Show issues
        if let Some(issues) = output.result.get("issues").and_then(|i| i.as_array()) {
            if !issues.is_empty() {
                println!("   │ Issues found: {}", issues.len());
                for issue in issues.iter().take(3) {
                    if let Some(issue_type) = issue.get("type").and_then(|t| t.as_str()) {
                        println!("   │   ⚠ {} ", issue_type);
                    }
                }
                if issues.len() > 3 {
                    println!("   │   ... and {} more", issues.len() - 3);
                }
            }
        }

        // Show vulnerabilities
        if let Some(vulns) = output.result.get("vulnerabilities").and_then(|v| v.as_array()) {
            if !vulns.is_empty() {
                println!("   │ Vulnerabilities: {}", vulns.len());
                for vuln in vulns.iter().take(3) {
                    if let Some(vuln_type) = vuln.get("type").and_then(|t| t.as_str()) {
                        let severity = vuln.get("severity").and_then(|s| s.as_str()).unwrap_or("unknown");
                        let severity_icon = match severity {
                            "critical" => "🔴",
                            "high" => "🟠",
                            "medium" => "🟡",
                            _ => "⚪",
                        };
                        println!("   │   {} [{}] {}", severity_icon, severity, vuln_type);
                    }
                }
            }
        }

        println!("   ╰────────────────────────────────────");
    }

    // Show final summary
    println!();
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("                    📈 FINAL SUMMARY                         ");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!();

    if let Some(overall) = result.summary.get("overall_score").and_then(|s| s.as_u64()) {
        let bar = "█".repeat((overall / 10) as usize);
        let empty = "░".repeat(10 - (overall / 10) as usize);
        println!("   Overall Score: [{}{}] {}/100", bar, empty, overall);

        let rating = match overall {
            90..=100 => "⭐⭐⭐⭐⭐ Excellent",
            70..=89 => "⭐⭐⭐⭐ Good",
            50..=69 => "⭐⭐⭐ Fair",
            30..=49 => "⭐⭐ Needs Improvement",
            _ => "⭐ Poor",
        };
        println!("   Rating: {}", rating);
    }

    if let Some(domain_scores) = result.summary.get("domain_scores").and_then(|d| d.as_object()) {
        println!();
        println!("   Domain Breakdown:");
        for (domain, score) in domain_scores {
            if let Some(score_val) = score.as_u64() {
                println!("     • {}: {}/100", domain, score_val);
            }
        }
    }

    if let Some(recommendations) = result.summary.get("recommendations").and_then(|r| r.as_array()) {
        println!();
        println!("   💡 Recommendations:");
        for rec in recommendations {
            if let Some(rec_text) = rec.as_str() {
                println!("     • {}", rec_text);
            }
        }
    }

    println!();
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("                    ✅ ANALYSIS COMPLETE                     ");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!();
    println!("   Agents involved: {}", result.agent_outputs.len());
    println!("   Total issues found: {}", result.summary.get("total_issues").and_then(|t| t.as_u64()).unwrap_or(0));
    println!("   Task ID: {}", result.task_id);
    println!();

    Ok(())
}
