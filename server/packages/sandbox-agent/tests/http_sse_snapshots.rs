use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::Router;
use futures::StreamExt;
use http_body_util::BodyExt;
use serde_json::{json, Map, Value};
use tempfile::TempDir;

use sandbox_agent_agent_management::agents::{AgentId, AgentManager};
use sandbox_agent_agent_management::testing::{test_agents_from_env, TestAgentConfig};
use sandbox_agent_agent_credentials::ExtractedCredentials;
use sandbox_agent_core::router::{build_router, AppState, AuthConfig};
use tower::ServiceExt;

const PROMPT: &str = "Reply with exactly the single word OK.";

struct TestApp {
    app: Router,
    _install_dir: TempDir,
}

impl TestApp {
    fn new() -> Self {
        let install_dir = tempfile::tempdir().expect("create temp install dir");
        let manager = AgentManager::new(install_dir.path())
            .expect("create agent manager");
        let state = AppState::new(AuthConfig::disabled(), manager);
        let app = build_router(state);
        Self {
            app,
            _install_dir: install_dir,
        }
    }
}

struct EnvGuard {
    saved: BTreeMap<String, Option<String>>,
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (key, value) in &self.saved {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }
}

fn apply_credentials(creds: &ExtractedCredentials) -> EnvGuard {
    let keys = ["ANTHROPIC_API_KEY", "CLAUDE_API_KEY", "OPENAI_API_KEY", "CODEX_API_KEY"];
    let mut saved = BTreeMap::new();
    for key in keys {
        saved.insert(key.to_string(), std::env::var(key).ok());
    }

    match creds.anthropic.as_ref() {
        Some(cred) => {
            std::env::set_var("ANTHROPIC_API_KEY", &cred.api_key);
            std::env::set_var("CLAUDE_API_KEY", &cred.api_key);
        }
        None => {
            std::env::remove_var("ANTHROPIC_API_KEY");
            std::env::remove_var("CLAUDE_API_KEY");
        }
    }

    match creds.openai.as_ref() {
        Some(cred) => {
            std::env::set_var("OPENAI_API_KEY", &cred.api_key);
            std::env::set_var("CODEX_API_KEY", &cred.api_key);
        }
        None => {
            std::env::remove_var("OPENAI_API_KEY");
            std::env::remove_var("CODEX_API_KEY");
        }
    }

    EnvGuard { saved }
}

async fn send_json(app: &Router, method: Method, path: &str, body: Option<Value>) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(path);
    let body = if let Some(body) = body {
        builder = builder.header("content-type", "application/json");
        Body::from(body.to_string())
    } else {
        Body::empty()
    };
    let request = builder.body(body).expect("request");
    let response = app
        .clone()
        .oneshot(request)
        .await
        .expect("request handled");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("read body")
        .to_bytes();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::String(String::from_utf8_lossy(&bytes).to_string()))
    };
    (status, value)
}

async fn send_status(app: &Router, method: Method, path: &str, body: Option<Value>) -> StatusCode {
    let (status, _) = send_json(app, method, path, body).await;
    status
}

async fn install_agent(app: &Router, agent: AgentId) {
    let status = send_status(
        app,
        Method::POST,
        &format!("/v1/agents/{}/install", agent.as_str()),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "install {agent}");
}

async fn create_session(app: &Router, agent: AgentId, session_id: &str) {
    let status = send_status(
        app,
        Method::POST,
        &format!("/v1/sessions/{session_id}"),
        Some(json!({
            "agent": agent.as_str(),
            "permissionMode": "bypass"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "create session {agent}");
}

async fn send_message(app: &Router, session_id: &str) {
    let status = send_status(
        app,
        Method::POST,
        &format!("/v1/sessions/{session_id}/messages"),
        Some(json!({ "message": PROMPT })),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "send message");
}

async fn poll_events_until(
    app: &Router,
    session_id: &str,
    timeout: Duration,
) -> Vec<Value> {
    let start = Instant::now();
    let mut offset = 0u64;
    let mut events = Vec::new();
    while start.elapsed() < timeout {
        let path = format!("/v1/sessions/{session_id}/events?offset={offset}&limit=200");
        let (status, payload) = send_json(app, Method::GET, &path, None).await;
        assert_eq!(status, StatusCode::OK, "poll events");
        let new_events = payload
            .get("events")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if !new_events.is_empty() {
            if let Some(last) = new_events.last().and_then(|event| event.get("id")).and_then(Value::as_u64) {
                offset = last;
            }
            events.extend(new_events);
            if should_stop(&events) {
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(800)).await;
    }
    events
}

async fn read_sse_events(
    app: &Router,
    session_id: &str,
    timeout: Duration,
) -> Vec<Value> {
    let request = Request::builder()
        .method(Method::GET)
        .uri(format!("/v1/sessions/{session_id}/events/sse?offset=0"))
        .body(Body::empty())
        .expect("sse request");
    let response = app
        .clone()
        .oneshot(request)
        .await
        .expect("sse response");
    assert_eq!(response.status(), StatusCode::OK, "sse status");

    let mut stream = response.into_body().into_data_stream();
    let mut buffer = String::new();
    let mut events = Vec::new();
    let start = Instant::now();
    loop {
        let remaining = match timeout.checked_sub(start.elapsed()) {
            Some(remaining) if !remaining.is_zero() => remaining,
            _ => break,
        };
        let next = tokio::time::timeout(remaining, stream.next()).await;
        let chunk = match next {
            Ok(Some(Ok(chunk))) => chunk,
            Ok(Some(Err(_))) => break,
            Ok(None) => break,
            Err(_) => break,
        };
        buffer.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(idx) = buffer.find("\n\n") {
            let block = buffer[..idx].to_string();
            buffer = buffer[idx + 2..].to_string();
            if let Some(event) = parse_sse_block(&block) {
                events.push(event);
            }
        }
        if should_stop(&events) {
            break;
        }
    }
    events
}

fn parse_sse_block(block: &str) -> Option<Value> {
    let mut data_lines = Vec::new();
    for line in block.lines() {
        if let Some(rest) = line.strip_prefix("data:") {
            data_lines.push(rest.trim_start());
        }
    }
    if data_lines.is_empty() {
        return None;
    }
    let data = data_lines.join("\n");
    serde_json::from_str(&data).ok()
}

fn should_stop(events: &[Value]) -> bool {
    events.iter().any(|event| is_assistant_message(event) || is_error_event(event))
}

fn is_assistant_message(event: &Value) -> bool {
    event
        .get("data")
        .and_then(|data| data.get("message"))
        .and_then(|message| message.get("role"))
        .and_then(Value::as_str)
        .map(|role| role == "assistant")
        .unwrap_or(false)
}

fn is_error_event(event: &Value) -> bool {
    event
        .get("data")
        .and_then(|data| data.get("error"))
        .is_some()
}

fn normalize_events(events: &[Value]) -> Value {
    let normalized = events
        .iter()
        .enumerate()
        .map(|(idx, event)| normalize_event(event, idx + 1))
        .collect::<Vec<_>>();
    Value::Array(normalized)
}

fn normalize_event(event: &Value, seq: usize) -> Value {
    let mut map = Map::new();
    map.insert("seq".to_string(), Value::Number(seq.into()));
    if let Some(agent) = event.get("agent").and_then(Value::as_str) {
        map.insert("agent".to_string(), Value::String(agent.to_string()));
    }
    let data = event.get("data").unwrap_or(&Value::Null);
    if let Some(message) = data.get("message") {
        map.insert("kind".to_string(), Value::String("message".to_string()));
        map.insert("message".to_string(), normalize_message(message));
    } else if let Some(started) = data.get("started") {
        map.insert("kind".to_string(), Value::String("started".to_string()));
        map.insert("started".to_string(), normalize_started(started));
    } else if let Some(error) = data.get("error") {
        map.insert("kind".to_string(), Value::String("error".to_string()));
        map.insert("error".to_string(), normalize_error(error));
    } else if let Some(question) = data.get("questionAsked") {
        map.insert("kind".to_string(), Value::String("question".to_string()));
        map.insert("question".to_string(), normalize_question(question));
    } else if let Some(permission) = data.get("permissionAsked") {
        map.insert("kind".to_string(), Value::String("permission".to_string()));
        map.insert("permission".to_string(), normalize_permission(permission));
    } else {
        map.insert("kind".to_string(), Value::String("unknown".to_string()));
    }
    Value::Object(map)
}

fn normalize_message(message: &Value) -> Value {
    let mut map = Map::new();
    if let Some(role) = message.get("role").and_then(Value::as_str) {
        map.insert("role".to_string(), Value::String(role.to_string()));
    }
    if let Some(parts) = message.get("parts").and_then(Value::as_array) {
        let parts = parts.iter().map(normalize_part).collect::<Vec<_>>();
        map.insert("parts".to_string(), Value::Array(parts));
    } else if message.get("raw").is_some() {
        map.insert("unparsed".to_string(), Value::Bool(true));
    }
    Value::Object(map)
}

fn normalize_part(part: &Value) -> Value {
    let mut map = Map::new();
    if let Some(part_type) = part.get("type").and_then(Value::as_str) {
        map.insert("type".to_string(), Value::String(part_type.to_string()));
    }
    if let Some(name) = part.get("name").and_then(Value::as_str) {
        map.insert("name".to_string(), Value::String(name.to_string()));
    }
    if part.get("text").is_some() {
        map.insert("text".to_string(), Value::String("<redacted>".to_string()));
    }
    if part.get("input").is_some() {
        map.insert("input".to_string(), Value::Bool(true));
    }
    if part.get("output").is_some() {
        map.insert("output".to_string(), Value::Bool(true));
    }
    Value::Object(map)
}

fn normalize_started(started: &Value) -> Value {
    let mut map = Map::new();
    if let Some(message) = started.get("message").and_then(Value::as_str) {
        map.insert("message".to_string(), Value::String(message.to_string()));
    }
    Value::Object(map)
}

fn normalize_error(error: &Value) -> Value {
    let mut map = Map::new();
    if let Some(kind) = error.get("kind").and_then(Value::as_str) {
        map.insert("kind".to_string(), Value::String(kind.to_string()));
    }
    if let Some(message) = error.get("message").and_then(Value::as_str) {
        map.insert("message".to_string(), Value::String(message.to_string()));
    }
    Value::Object(map)
}

fn normalize_question(question: &Value) -> Value {
    let mut map = Map::new();
    if question.get("id").is_some() {
        map.insert("id".to_string(), Value::String("<redacted>".to_string()));
    }
    if let Some(questions) = question.get("questions").and_then(Value::as_array) {
        map.insert("count".to_string(), Value::Number(questions.len().into()));
    }
    Value::Object(map)
}

fn normalize_permission(permission: &Value) -> Value {
    let mut map = Map::new();
    if permission.get("id").is_some() {
        map.insert("id".to_string(), Value::String("<redacted>".to_string()));
    }
    if let Some(value) = permission.get("permission").and_then(Value::as_str) {
        map.insert("permission".to_string(), Value::String(value.to_string()));
    }
    Value::Object(map)
}

fn snapshot_name(prefix: &str, agent: AgentId) -> String {
    format!("{prefix}_{}", agent.as_str())
}

async fn run_http_events_snapshot(app: &Router, config: &TestAgentConfig) {
    let _guard = apply_credentials(&config.credentials);
    install_agent(app, config.agent).await;

    let session_id = format!("session-{}", config.agent.as_str());
    create_session(app, config.agent, &session_id).await;
    send_message(app, &session_id).await;

    let events = poll_events_until(app, &session_id, Duration::from_secs(120)).await;
    assert!(
        !events.is_empty(),
        "no events collected for {}",
        config.agent
    );
    assert!(
        should_stop(&events),
        "timed out waiting for assistant/error event for {}",
        config.agent
    );
    let normalized = normalize_events(&events);
    insta::with_settings!({
        snapshot_suffix => snapshot_name("http_events", config.agent),
    }, {
        insta::assert_yaml_snapshot!(normalized);
    });
}

async fn run_sse_events_snapshot(app: &Router, config: &TestAgentConfig) {
    let _guard = apply_credentials(&config.credentials);
    install_agent(app, config.agent).await;

    let session_id = format!("sse-{}", config.agent.as_str());
    create_session(app, config.agent, &session_id).await;

    let sse_task = {
        let app = app.clone();
        let session_id = session_id.clone();
        tokio::spawn(async move {
            read_sse_events(&app, &session_id, Duration::from_secs(120)).await
        })
    };

    send_message(app, &session_id).await;

    let events = sse_task.await.expect("sse task");
    assert!(
        !events.is_empty(),
        "no sse events collected for {}",
        config.agent
    );
    assert!(
        should_stop(&events),
        "timed out waiting for assistant/error event for {}",
        config.agent
    );
    let normalized = normalize_events(&events);
    insta::with_settings!({
        snapshot_suffix => snapshot_name("sse_events", config.agent),
    }, {
        insta::assert_yaml_snapshot!(normalized);
    });
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn http_events_snapshots() {
    let configs = test_agents_from_env().expect("configure SANDBOX_TEST_AGENTS");
    let app = TestApp::new();
    for config in &configs {
        run_http_events_snapshot(&app.app, config).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sse_events_snapshots() {
    let configs = test_agents_from_env().expect("configure SANDBOX_TEST_AGENTS");
    let app = TestApp::new();
    for config in &configs {
        run_sse_events_snapshot(&app.app, config).await;
    }
}
