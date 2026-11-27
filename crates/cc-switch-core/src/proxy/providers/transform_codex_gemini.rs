//! Codex Responses ↔ Gemini Native conversion through the shared message model.

use super::{gemini_shadow::GeminiShadowStore, transform_codex_chat::CodexToolContext};
use crate::proxy::error::ProxyError;
use serde_json::Value;
use std::sync::Arc;
use bytes::Bytes;
use futures::Stream;

pub fn responses_to_gemini(
    body: Value,
    shadow: Option<&GeminiShadowStore>,
    provider: Option<&str>,
    session: Option<&str>,
) -> Result<Value, ProxyError> {
    let message = super::transform_codex_anthropic::responses_request_to_anthropic(
        body, super::transform_codex_anthropic::DEFAULT_CODEX_ANTHROPIC_MAX_TOKENS,
    )?;
    super::transform_gemini::anthropic_to_gemini_with_shadow(message, shadow, provider, session)
}

pub fn gemini_to_responses(
    body: Value,
    shadow: Option<&GeminiShadowStore>,
    provider: Option<&str>,
    session: Option<&str>,
    tools: &CodexToolContext,
) -> Result<Value, ProxyError> {
    let message = super::transform_gemini::gemini_to_anthropic_with_shadow_and_hints(
        body, shadow, provider, session, None,
    )?;
    super::transform_codex_anthropic::anthropic_response_to_responses_with_context(message, tools)
}

pub fn gemini_stream_to_responses<E: std::error::Error + Send + 'static>(
    stream: impl Stream<Item = Result<Bytes, E>> + Send + 'static,
    shadow: Option<Arc<GeminiShadowStore>>,
    provider: Option<String>,
    session: Option<String>,
    tools: CodexToolContext,
) -> impl Stream<Item = Result<Bytes, std::io::Error>> + Send {
    let messages = super::streaming_gemini::create_anthropic_sse_stream_from_gemini(
        stream, shadow, provider, session, None,
    );
    super::streaming_codex_anthropic::create_responses_sse_stream_from_anthropic_with_context(messages, tools)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use futures::StreamExt;
    use bytes::Bytes;

    #[test]
    fn request_preserves_instructions_text_tools_and_output_limit() {
        let body = json!({
            "model":"gemini-3.6-flash", "instructions":"Be concise", "input":"Hello",
            "max_output_tokens":2048,
            "tools":[{"type":"function","name":"read_file","description":"Read a file","parameters":{"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}}]
        });
        let converted = responses_to_gemini(body, None, None, None).unwrap();
        assert_eq!(converted["systemInstruction"]["parts"][0]["text"], "Be concise");
        assert_eq!(converted["contents"][0]["parts"][0]["text"], "Hello");
        assert_eq!(converted["generationConfig"]["maxOutputTokens"], 2048);
        assert_eq!(converted["tools"][0]["functionDeclarations"][0]["name"], "read_file");
        assert!(converted.get("input").is_none());
    }

    #[test]
    fn response_preserves_tool_calls_and_usage() {
        let body = json!({"responseId":"g1","modelVersion":"gemini-3.6-flash",
            "candidates":[{"content":{"parts":[{"functionCall":{"id":"call_1","name":"read_file","args":{"path":"a.rs"}}}]},"finishReason":"STOP"}],
            "usageMetadata":{"promptTokenCount":100,"candidatesTokenCount":12,"cachedContentTokenCount":20,"totalTokenCount":112}});
        let response=gemini_to_responses(body, None, None, None, &Default::default()).unwrap();
        assert_eq!(response["output"][0]["type"],"function_call");
        assert_eq!(response["output"][0]["name"],"read_file");
        assert_eq!(response["usage"]["input_tokens"],100);
        assert_eq!(response["usage"]["output_tokens"],12);
        assert_eq!(response["usage"]["input_tokens_details"]["cached_tokens"],20);
    }

    #[tokio::test]
    async fn fragmented_stream_emits_a_completed_responses_event() {
        let sse="data: {\"responseId\":\"g1\",\"modelVersion\":\"gemini-3.6-flash\",\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"Hello\"}]},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":10,\"candidatesTokenCount\":2}}\n\n";
        let chunks=sse.as_bytes().chunks(7).map(|c| Ok::<_,std::io::Error>(Bytes::copy_from_slice(c))).collect::<Vec<_>>();
        let stream=gemini_stream_to_responses(futures::stream::iter(chunks),None,None,None,Default::default());
        futures::pin_mut!(stream);
        let mut output=String::new();
        while let Some(chunk)=stream.next().await { output.push_str(std::str::from_utf8(&chunk.unwrap()).unwrap()); }
        assert!(output.contains("response.output_text.delta"));
        assert!(output.contains("Hello"));
        assert!(output.contains("response.completed"));
        assert!(output.contains("\"input_tokens\":10"));
    }

    #[test]
    fn tool_result_replays_the_google_thought_signature() {
        let shadow = GeminiShadowStore::new();
        let response = gemini_to_responses(json!({
            "modelVersion": "gemini-flash",
            "candidates": [{"content": {"parts": [{
                "functionCall": {"id": "google-call", "name": "read_file", "args": {"path": "a.rs"}},
                "thoughtSignature": "fixture-signature"
            }]}, "finishReason": "STOP"}]
        }), Some(&shadow), Some("relay"), Some("session"), &Default::default()).unwrap();
        let call = response["output"][0].clone();
        let converted = responses_to_gemini(json!({
            "model": "gemini-flash",
            "input": [
                {"role": "user", "content": "Read a.rs"},
                call,
                {"type": "function_call_output", "call_id": call["call_id"], "output": "file contents"}
            ]
        }), Some(&shadow), Some("relay"), Some("session")).unwrap();
        let parts: Vec<&Value> = converted["contents"].as_array().unwrap().iter()
            .flat_map(|turn| turn["parts"].as_array().unwrap()).collect();
        let replay = parts.iter().find(|part| part.get("functionCall").is_some()).unwrap();
        assert_eq!(replay["thoughtSignature"], "fixture-signature");
        let result = parts.iter().find(|part| part.get("functionResponse").is_some()).unwrap();
        assert_eq!(result["functionResponse"]["name"], "read_file");
    }
}
