#[cfg(test)]
mod tests {
    use crate::ai::client::{ChatResponse, Message, Content, ToolCall, ToolFunction};

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

    // ===== Responses API Tests =====

    #[test]
    fn test_convert_messages_to_responses_input() {
        use crate::ai::client::convert_messages_to_responses_input;

        let messages = vec![
            Message {
                role: "system".to_string(),
                content: Some(Content::Simple("You are a helpful assistant.".to_string())),
                tool_calls: None,
                tool_call_id: None,
            },
            Message {
                role: "system".to_string(),
                content: Some(Content::Simple("Extra context info.".to_string())),
                tool_calls: None,
                tool_call_id: None,
            },
            Message {
                role: "user".to_string(),
                content: Some(Content::Simple("Hello!".to_string())),
                tool_calls: None,
                tool_call_id: None,
            },
        ];

        let (instructions, items) = convert_messages_to_responses_input(&messages);
        
        // First system message becomes instructions
        assert_eq!(instructions, Some("You are a helpful assistant.".to_string()));
        
        // Second system + user = 2 items
        assert_eq!(items.len(), 2);
        
        // Verify serialization works
        let json = serde_json::to_string(&items).unwrap();
        assert!(json.contains("developer"));
        assert!(json.contains("Hello!"));
    }

    #[test]
    fn test_convert_messages_with_tool_calls() {
        use crate::ai::client::convert_messages_to_responses_input;

        let messages = vec![
            Message {
                role: "assistant".to_string(),
                content: Some(Content::Simple("Let me search.".to_string())),
                tool_calls: Some(vec![ToolCall {
                    id: "call_abc123".to_string(),
                    r#type: "function".to_string(),
                    function: ToolFunction {
                        name: "search".to_string(),
                        arguments: r#"{"query": "test"}"#.to_string(),
                    },
                }]),
                tool_call_id: None,
            },
            Message {
                role: "tool".to_string(),
                content: Some(Content::Simple("Search result: found it".to_string())),
                tool_calls: None,
                tool_call_id: Some("call_abc123".to_string()),
            },
        ];

        let (instructions, items) = convert_messages_to_responses_input(&messages);
        assert!(instructions.is_none());
        
        // assistant content + function_call + function_call_output = 3 items
        assert_eq!(items.len(), 3);
        
        let json = serde_json::to_string(&items).unwrap();
        assert!(json.contains("function_call"));
        assert!(json.contains("function_call_output"));
        assert!(json.contains("call_abc123"));
    }

    #[test]
    fn test_convert_responses_output_text() {
        use crate::ai::client::convert_responses_output_to_message;
        use crate::ai::client::{ResponsesOutputItem, ResponsesContentBlock};

        let output = vec![ResponsesOutputItem::Message {
            content: vec![ResponsesContentBlock::OutputText {
                text: "Hello from Responses API!".to_string(),
            }],
        }];

        let msg = convert_responses_output_to_message(&output);
        assert_eq!(msg.role, "assistant");
        assert_eq!(msg.content_as_str(), "Hello from Responses API!");
        assert!(msg.tool_calls.is_none());
    }

    #[test]
    fn test_convert_responses_output_function_call() {
        use crate::ai::client::convert_responses_output_to_message;
        use crate::ai::client::{ResponsesOutputItem, ResponsesContentBlock};

        let output = vec![
            ResponsesOutputItem::Message {
                content: vec![ResponsesContentBlock::OutputText {
                    text: "Thinking...".to_string(),
                }],
            },
            ResponsesOutputItem::FunctionCall {
                id: "fc_001".to_string(),
                call_id: "call_xyz789".to_string(),
                name: "get_weather".to_string(),
                arguments: r#"{"city": "Tokyo"}"#.to_string(),
            },
        ];

        let msg = convert_responses_output_to_message(&output);
        assert_eq!(msg.role, "assistant");
        assert_eq!(msg.content_as_str(), "Thinking...");
        assert!(msg.tool_calls.is_some());
        let calls = msg.tool_calls.unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call_xyz789");
        assert_eq!(calls[0].function.name, "get_weather");
    }

    #[test]
    fn test_convert_responses_output_empty() {
        use crate::ai::client::convert_responses_output_to_message;

        let output = vec![];
        let msg = convert_responses_output_to_message(&output);
        assert_eq!(msg.role, "assistant");
        assert!(msg.content.is_none());
        assert!(msg.tool_calls.is_none());
    }
}
