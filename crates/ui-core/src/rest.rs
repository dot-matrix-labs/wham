use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Request {
    pub id: u64,
    pub method: HttpMethod,
    pub url: String,
    pub body: Option<serde_json::Value>,
    pub timeout_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Response {
    pub status: u16,
    pub body: serde_json::Value,
}

#[derive(Clone, Debug)]
pub struct RetryPolicy {
    pub max_retries: u8,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
}

impl RetryPolicy {
    pub fn default_fast() -> Self {
        Self {
            max_retries: 2,
            base_delay_ms: 250,
            max_delay_ms: 1500,
        }
    }

    pub fn delay_for_attempt(&self, attempt: u8) -> Duration {
        let factor = 2u64.saturating_pow(attempt as u32);
        let delay = self.base_delay_ms.saturating_mul(factor).min(self.max_delay_ms);
        Duration::from_millis(delay)
    }
}

pub trait HttpClient {
    fn request(&mut self, request: Request) -> Result<Response, String>;
}

