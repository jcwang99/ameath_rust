#[cfg(test)]
mod tests {
    use crate::ai::client::{parse_chat_response_text, parse_message_from_value, Content, Message};
    use serde_json::json;

    #[test]
    fn test_parse_tool_call_response_with_null_content() {
        let json = r#"{
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": null,
                        "tool_calls": [
                            {
                                "id": "call_123",
                                "type": "function",
                                "function": {
                                    "name": "test_tool",
                                    "arguments": "{}"
                                }
                            }
                        ]
                    }
                }
            ]
        }"#;

        let msg = parse_chat_response_text(json).unwrap();
        assert_eq!(msg.role, "assistant");
        assert!(msg.content.is_none());
        assert!(msg.tool_calls.is_some());
        assert_eq!(msg.content_as_str(), "");
    }

    #[test]
    fn test_parse_simple_message() {
        let json = r#"{
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "Hello world"
                    }
                }
            ]
        }"#;

        let msg = parse_chat_response_text(json).unwrap();
        assert_eq!(msg.role, "assistant");
        assert!(msg.content.is_some());
        assert_eq!(msg.content_as_str(), "Hello world");
    }

    #[test]
    fn test_parse_multimodal_message_concatenates_text() {
        let value = json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": [
                            { "type": "text", "text": "Hello" },
                            { "type": "text", "text": " world" },
                            { "type": "image_url", "image_url": { "url": "https://example.com/a.png" } }
                        ]
                    }
                }
            ]
        });

        let msg = parse_message_from_value(&value).unwrap();
        assert_eq!(msg.content_as_str(), "Hello\n world");
        assert!(matches!(msg.content, Some(Content::Multimodal(_))));
    }

    #[test]
    fn test_parse_tool_call_arguments_from_object() {
        let value = json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "tool_calls": [
                            {
                                "id": "call_1",
                                "type": "function",
                                "function": {
                                    "name": "search",
                                    "arguments": { "query": "rust" }
                                }
                            }
                        ]
                    }
                }
            ]
        });

        let msg = parse_message_from_value(&value).unwrap();
        let tool_calls = msg.tool_calls.unwrap();
        assert_eq!(tool_calls[0].function.name, "search");
        assert_eq!(tool_calls[0].function.arguments, r#"{"query":"rust"}"#);
    }

    #[test]
    fn test_message_content_as_str_empty_when_none() {
        let msg = Message {
            role: "assistant".to_string(),
            content: None,
            tool_calls: None,
            tool_call_id: None,
        };

        assert_eq!(msg.content_as_str(), "");
    }
}
