//! BOPTEST REST client (the web service from BOPTEST's docker compose). Replies are
//! `{status, message, payload}`; an empty `advance` payload means the scenario has ended.

use std::time::Duration;

use serde_json::{Value, json};

use crate::plant::{Measurements, Plant, PlantError, Writes};

/// Per request; a hung advance should fail in minutes, not hours.
const STEP_TIMEOUT: Duration = Duration::from_secs(300);
/// Initialization runs the whole warm-up inside one request, and select waits for a free worker
/// (an abandoned select still creates a test, which then holds that worker).
const INIT_TIMEOUT: Duration = Duration::from_secs(3600);

/// A selected BOPTEST test, ready to initialize and advance.
#[derive(Debug)]
pub struct BoptestClient {
    http: reqwest::Client,
    base: String,
    testid: String,
}

fn transport(e: reqwest::Error) -> PlantError {
    PlantError::Transport {
        message: e.without_url().to_string(),
    }
}

/// The reply text, after checking the HTTP status. A refusal keeps its status even when the
/// body is not JSON (the message is then the start of the raw text).
async fn send_text(request: reqwest::RequestBuilder) -> Result<String, PlantError> {
    let response = request.send().await.map_err(transport)?;
    let status = response.status().as_u16();
    let text = response.text().await.map_err(transport)?;
    if !(200..300).contains(&status) {
        let message = serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|b| b.get("message").and_then(Value::as_str).map(str::to_owned))
            .unwrap_or_else(|| text.chars().take(200).collect());
        return Err(PlantError::Refused { status, message });
    }
    Ok(text)
}

/// The whole JSON reply, after checking the HTTP status.
async fn send_raw(request: reqwest::RequestBuilder) -> Result<Value, PlantError> {
    let text = send_text(request).await?;
    serde_json::from_str(&text).map_err(|_| PlantError::Malformed {
        message: text.chars().take(200).collect(),
    })
}

/// The reply's `payload`; a reply without one is a protocol mismatch, not an end of scenario.
async fn send(request: reqwest::RequestBuilder) -> Result<Value, PlantError> {
    let mut body = send_raw(request).await?;
    body.get_mut("payload")
        .map(Value::take)
        .ok_or_else(|| PlantError::Malformed {
            message: "reply has no payload".into(),
        })
}

/// Numeric readings from a payload object. BOPTEST sends numbers (booleans as 0/1); anything
/// else is skipped rather than guessed, so a sequence that needs it degrades visibly. An empty
/// object means the scenario has ended; anything that is not an object is malformed.
fn measurements(payload: &Value) -> Result<Measurements, PlantError> {
    let object = payload.as_object().ok_or_else(|| PlantError::Malformed {
        message: "payload is not an object".into(),
    })?;
    if object.is_empty() {
        return Err(PlantError::Ended);
    }
    Ok(object
        .iter()
        .filter_map(|(k, v)| Some((k.clone(), v.as_f64()?)))
        .collect())
}

impl BoptestClient {
    /// Selects a test case on the service at `base` (BOPTEST v0.9.0's compose serves `http://127.0.0.1:80`).
    pub async fn select(base: &str, testcase: &str) -> Result<Self, PlantError> {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(STEP_TIMEOUT)
            .build()
            .map_err(transport)?;
        let base = base.trim_end_matches('/').to_owned();
        let select = http
            .post(format!("{base}/testcases/{testcase}/select"))
            .timeout(INIT_TIMEOUT);
        let body = send_raw(select).await?;
        let testid = body
            .get("testid")
            .and_then(Value::as_str)
            .ok_or_else(|| PlantError::Malformed {
                message: "select reply has no testid".into(),
            })?
            .to_owned();
        Ok(Self { http, base, testid })
    }

    /// `select`, retried while BOPTEST answers 404: the test case is not uploaded yet (a fresh
    /// deployment is still provisioning). A 404 creates no test, so a retry cannot leave an
    /// orphan; any other error is returned at once.
    pub async fn select_when_ready(
        base: &str,
        testcase: &str,
        attempts: u32,
        delay: Duration,
    ) -> Result<Self, PlantError> {
        let mut left = attempts.max(1);
        loop {
            left -= 1;
            match Self::select(base, testcase).await {
                Err(PlantError::Refused { status: 404, .. }) if left > 0 => {
                    tokio::time::sleep(delay).await;
                }
                other => return other,
            }
        }
    }

    pub fn testid(&self) -> &str {
        &self.testid
    }

    /// Starts at `start_time` after `warmup` seconds under baseline control; returns the first
    /// readings.
    pub async fn initialize(
        &self,
        start_time: f64,
        warmup: f64,
    ) -> Result<Measurements, PlantError> {
        let url = format!("{}/initialize/{}", self.base, self.testid);
        let body = json!({"start_time": start_time, "warmup_period": warmup});
        let request = self.http.put(url).json(&body).timeout(INIT_TIMEOUT);
        match measurements(&send(request).await?) {
            Err(PlantError::Ended) => Err(PlantError::Malformed {
                message: "initialize returned no readings".into(),
            }),
            other => other,
        }
    }

    pub async fn set_step(&self, seconds: f64) -> Result<(), PlantError> {
        let url = format!("{}/step/{}", self.base, self.testid);
        send(self.http.put(url).json(&json!({"step": seconds})))
            .await
            .map(|_| ())
    }

    /// KPIs for the run so far (`ener_tot`, `cost_tot`, `tdis_tot`, ...).
    pub async fn kpi(&self) -> Result<Measurements, PlantError> {
        let url = format!("{}/kpi/{}", self.base, self.testid);
        let payload = send(self.http.get(url)).await?;
        Ok(payload
            .as_object()
            .map(|o| {
                o.iter()
                    .filter_map(|(k, v)| Some((k.clone(), v.as_f64()?)))
                    .collect()
            })
            .unwrap_or_default())
    }

    pub async fn stop(&self) -> Result<(), PlantError> {
        let url = format!("{}/stop/{}", self.base, self.testid);
        // v0.9.0 answers a stop with a plain `OK`, not the usual JSON envelope.
        send_text(self.http.put(url)).await.map(|_| ())
    }
}

impl Plant for BoptestClient {
    async fn advance(&mut self, writes: &Writes) -> Result<Measurements, PlantError> {
        let url = format!("{}/advance/{}", self.base, self.testid);
        measurements(&send(self.http.post(url).json(writes)).await?)
    }
}
