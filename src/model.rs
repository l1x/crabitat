//! Model types and configuration

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::{CrabitatError, ModelError};

/// Represents an autonomous agent with specific capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct Model {
    pub id: String,
    pub name: String,
    pub temperature: f32,
    pub url: String,
}

impl fmt::Display for Model {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.name, self.id)
    }
}

impl Model {
    /// Get model details from Ollama API
    pub async fn show(&self) -> Result<ModelDetails, ModelError> {
        let client = reqwest::Client::new();

        let payload = serde_json::json!({
            "model": self.name,
            "temperature": self.temperature,
        });

        let response = client
            .post(&format!("{}/api/show", self.url))
            .json(&payload)
            .send()
            .await
            .map_err(|e| ModelError::ApiError(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(ModelError::HttpError(format!("{}: {}", status, text)));
        }

        let details: ModelDetails = response
            .json()
            .await
            .map_err(|e| ModelError::InvalidResponse(e.to_string()))?;

        Ok(details)
    }
}

/// Response from /api/show endpoint
#[derive(Debug, Deserialize)]
pub struct ModelDetails {
    pub modelfile: String,
    pub parameters: String,
    pub template: String,
    pub details: ModelInfo,
    pub model_info: ModelMetadata,
}

#[derive(Debug, Deserialize)]
pub struct ModelInfo {
    pub parent_model: String,
    pub format: String,
    pub family: String,
    pub families: Option<Vec<String>>,
    pub parameter_size: String,
    pub quantization_level: String,
}

#[derive(Debug, Deserialize)]
pub struct ModelMetadata {
    #[serde(flatten)]
    pub metadata: std::collections::HashMap<String, serde_json::Value>,
}

impl fmt::Display for ModelDetails {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Model: {}", self.details.family)?;
        writeln!(f, "  Family: {}", self.details.family)?;
        writeln!(f, "  Format: {}", self.details.format)?;
        writeln!(f, "  Size: {}", self.details.parameter_size)?;
        writeln!(f, "  Quantization: {}", self.details.quantization_level)?;

        // Extract key parameters from the raw string
        if let Some(temp) = extract_param(&self.parameters, "temperature") {
            writeln!(f, "  Temperature: {}", temp)?;
        }
        if let Some(top_k) = extract_param(&self.parameters, "top_k") {
            writeln!(f, "  Top-K: {}", top_k)?;
        }
        if let Some(top_p) = extract_param(&self.parameters, "top_p") {
            writeln!(f, "  Top-P: {}", top_p)?;
        }

        // Show context length if available
        if let Some(context_len) = self.model_info.metadata.get("gemma3.context_length") {
            writeln!(f, "  Context: {}", context_len)?;
        }

        Ok(())
    }
}

fn extract_param(params: &str, key: &str) -> Option<String> {
    params
        .lines()
        .find(|line| line.trim().starts_with(key))
        .and_then(|line| line.split_whitespace().nth(1))
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_model_show() {
        let model = Model {
            id: "test".to_string(),
            name: "gemma3:latest".to_string(),
            temperature: 0.5,
            url: "http://localhost:11434".to_string(),
        };

        // This would fail in CI without Ollama running, but shows the API
        // let details = model.show().await.unwrap();
        // assert!(!details.modelfile.is_empty());
    }
}
