use super::*;

#[test]
fn dotenv_config_validates_and_keeps_credentials_private() {
    let cfg = crate::config::Config::default();
    let text = "API_BASE_URL=https://example.test/v1\nAPI_KEY='test-only'\nAPI_MODEL=test-model\nAPI_TIMEOUT_SECS=45\nAPI_MAX_RETRIES=1\n";
    let provider = OpenAiProvider::from_env_text(&cfg, text).unwrap();
    assert_eq!(provider.base_url, "https://example.test/v1");
    assert_eq!(provider.model, "test-model");
    assert_eq!(provider.timeout_secs, 45);
    assert_eq!(provider.retries, 1);
    assert_eq!(provider.api_key.as_deref(), Some("test-only"));
    assert!(OpenAiProvider::from_env_text(&cfg, "API_KEY=").is_err());
    assert!(OpenAiProvider::from_env_text(&cfg, &text.replace("45", "0")).is_err());
    assert!(OpenAiProvider::from_env_text(&cfg, "API_KEY='unclosed").is_err());
}

#[test]
fn fake_provider_replays_scripted_turns() {
    let mut p = FakeProvider::new(vec![
        FakeTurn::Calls(vec![("write_design".into(), "{}".into())]),
        FakeTurn::Text("finished".into()),
    ]);
    let tools = vec![ToolSpec {
        name: "write_design".into(),
        description: "".into(),
        parameters: serde_json::json!({"type": "object"}),
    }];
    let r1 = p.complete(&[ChatMessage::user("go")], &tools).unwrap();
    assert_eq!(r1.tool_calls.len(), 1);
    let r2 = p.complete(&[ChatMessage::user("go")], &tools).unwrap();
    assert_eq!(r2.text, "finished");
    assert_eq!(p.cursor, 2);
}

#[test]
fn request_body_shape() {
    let msgs = vec![
        ChatMessage::system("sys"),
        ChatMessage::tool_result("t", "res"),
    ];
    let body = build_request_body(&msgs, &[], "m1");
    assert_eq!(body["messages"][1]["name"], "t");
    assert_eq!(body["model"], "m1");
}

#[test]
fn parse_openai_response() {
    let json = serde_json::json!({
        "choices": [{
            "message": {
                "content": "",
                "tool_calls": [{
                    "function": {"name": "assets.build_candidate", "arguments": "{\"recipe\":\"a.json\"}"}
                }]
            },
            "finish_reason": "tool_calls"
        }],
        "usage": {"prompt_tokens": 7, "completion_tokens": 3}
    });
    let r = parse_response(&json, "gpt").unwrap();
    assert_eq!(r.tool_calls[0].name, "assets.build_candidate");
    assert_eq!(r.prompt_tokens, 7);
    assert_eq!(r.stop_reason, "tool_calls");
}
