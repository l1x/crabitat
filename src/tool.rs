//! Tool trait for agent capabilities

use serde_json;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use crate::error::{OrchestratorError, OrchestratorResult};

/// Core trait for tools that agents can use
pub trait Tool: Send + Sync {
    /// Unique name identifier for the tool
    fn name(&self) -> &str;

    /// Description of what the tool does (for agent context)
    fn description(&self) -> &str;

    /// Execute the tool with given arguments
    fn execute(&self) -> OrchestratorResult<String>;

    /// Validate tool arguments before execution
    fn validate_arguments(&self) -> OrchestratorResult<()>;
}
