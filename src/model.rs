//! Model types and configuration

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::ModelError;

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
    ///
    /// Get model details from Ollama API
    ///
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

    ///
    /// Send chat completion request to Ollama
    ///
    pub async fn chat(&self, messages: Vec<ChatMessage>) -> Result<ChatResponse, ModelError> {
        let client = reqwest::Client::new();

        let payload = serde_json::json!({
            "model": self.name,
            "messages": messages,
            "stream": false,
            "options": {
                "temperature": self.temperature
            }
        });

        let response = client
            .post(&format!("{}/api/chat", self.url))
            .json(&payload)
            .send()
            .await
            .map_err(|e| ModelError::ApiError(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(ModelError::HttpError(format!("{}: {}", status, text)));
        }

        let chat_response: ChatResponse = response
            .json()
            .await
            .map_err(|e| ModelError::InvalidResponse(e.to_string()))?;

        Ok(chat_response)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageRole {
    #[serde(rename = "system")]
    System,
    #[serde(rename = "user")]
    User,
    #[serde(rename = "assistant")]
    Assistant,
}

/// Chat message for model interaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: String,
}

/// Response from /api/chat endpoint
#[derive(Debug, Deserialize)]
pub struct ChatResponseMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct ChatResponse {
    pub message: ChatResponseMessage,
    pub done: bool,
    #[serde(default)]
    pub total_duration: Option<u64>,
    #[serde(default)]
    pub prompt_eval_count: Option<u32>,
    #[serde(default)]
    pub eval_count: Option<u32>,
}

impl fmt::Display for ChatResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Response: {}", self.message.content)?;
        if let Some(duration) = self.total_duration {
            writeln!(f, "Duration: {}ms", duration / 1_000_000)?;
        }
        if let Some(prompt_tokens) = self.prompt_eval_count {
            if let Some(eval_tokens) = self.eval_count {
                writeln!(
                    f,
                    "Tokens: {} prompt + {} completion = {}",
                    prompt_tokens,
                    eval_tokens,
                    prompt_tokens + eval_tokens
                )?;
            }
        }
        Ok(())
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
