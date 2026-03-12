#[cfg(test)]
mod tests {
    use crate::ai::client::{ChatResponse, Message};

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

        let res: ChatResponse = serde_json::from_str(json).unwrap();
        let msg = &res.choices[0].message;
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

        let res: ChatResponse = serde_json::from_str(json).unwrap();
        let msg = &res.choices[0].message;
        assert_eq!(msg.role, "assistant");
        assert!(msg.content.is_some());
        assert_eq!(msg.content_as_str(), "Hello world");
    }
}
