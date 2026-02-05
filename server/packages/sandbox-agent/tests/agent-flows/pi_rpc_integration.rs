// Pi RPC integration tests (gated via SANDBOX_TEST_PI + PATH).
include!("../common/http.rs");

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pi_rpc_session_and_stream() {
    let configs = match test_agents_from_env() {
        Ok(configs) => configs,
        Err(err) => {
            eprintln!("Skipping Pi RPC integration test: {err}");
            return;
        }
    };
    let Some(config) = configs.iter().find(|config| config.agent == AgentId::Pi) else {
        return;
    };

    let app = TestApp::new();
    let _guard = apply_credentials(&config.credentials);
    install_agent(&app.app, config.agent).await;

    let session_id = "pi-rpc-session".to_string();
    let (status, payload) = send_json(
        &app.app,
        Method::POST,
        &format!("/v1/sessions/{session_id}"),
        Some(json!({
            "agent": "pi",
            "permissionMode": test_permission_mode(AgentId::Pi),
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "create pi session");
    let native_session_id = payload
        .get("native_session_id")
        .and_then(Value::as_str)
        .unwrap_or("");
    assert!(
        !native_session_id.is_empty(),
        "expected native_session_id for pi session"
    );

    let events = read_turn_stream_events(&app.app, &session_id, Duration::from_secs(120)).await;
    assert!(!events.is_empty(), "no events from pi stream");
    assert!(
        !events.iter().any(is_unparsed_event),
        "agent.unparsed event encountered"
    );

    let mut last_sequence = 0u64;
    for event in events {
        let sequence = event
            .get("sequence")
            .and_then(Value::as_u64)
            .expect("missing sequence");
        assert!(
            sequence > last_sequence,
            "sequence did not increase (prev {last_sequence}, next {sequence})"
        );
        last_sequence = sequence;
    }
}
