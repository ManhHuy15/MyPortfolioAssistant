use adapters::outbound::persistence::workers_ai::{WorkersAiSseLine, parse_workers_ai_sse_line};

#[test]
fn parse_workers_ai_sse_line_extracts_tokens_and_done() {
    assert_eq!(
        parse_workers_ai_sse_line(r#"data: {"response":"Xin ","p":"abc"}"#),
        WorkersAiSseLine::Token("Xin ".to_string())
    );
    assert_eq!(
        parse_workers_ai_sse_line("data: [DONE]"),
        WorkersAiSseLine::Done
    );
    assert_eq!(
        parse_workers_ai_sse_line(r#"data: {"response":""}"#),
        WorkersAiSseLine::Ignore
    );
    assert_eq!(parse_workers_ai_sse_line(""), WorkersAiSseLine::Ignore);
    assert_eq!(
        parse_workers_ai_sse_line(": keep-alive"),
        WorkersAiSseLine::Ignore
    );
}
