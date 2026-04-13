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
                ..Default::default()
            },
            Message {
                role: "system".to_string(),
                content: Some(Content::Simple("Extra context info.".to_string())),
                ..Default::default()
            },
            Message {
                role: "user".to_string(),
                content: Some(Content::Simple("Hello!".to_string())),
                ..Default::default()
            },
        ];

        let (instructions, items) = convert_messages_to_responses_input(&messages);
        
        // First system message becomes instructions
        assert_eq!(instructions, Some("You are a helpful assistant.".to_string()));
        
        // Second system + user = 2 items
        assert_eq!(items.len(), 2);
        
        // Verify serialization works
        let json = serde_json::to_string(&items).unwrap();
        assert!(json.contains("system"));
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
                ..Default::default()
            },
            Message {
                role: "tool".to_string(),
                content: Some(Content::Simple("Search result: found it".to_string())),
                tool_call_id: Some("call_abc123".to_string()),
                ..Default::default()
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
                item_id: "msg_xyz".to_string(),
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

    #[test]
    fn test_parse_responses_api_output_full_json() {
        use crate::ai::client::parse_responses_api_output;

        // Simulate a real OpenAI Responses API response with extra fields
        let json = r#"{
            "id": "resp_abc123",
            "object": "response",
            "created_at": 1700000000,
            "status": "completed",
            "model": "gpt-4o",
            "instructions": "You are helpful.",
            "output": [
                {
                    "id": "msg_001",
                    "type": "message",
                    "role": "assistant",
                    "status": "completed",
                    "content": [
                        {
                            "type": "output_text",
                            "text": "Hello!",
                            "annotations": []
                        }
                    ]
                }
            ],
            "usage": {"input_tokens": 10, "output_tokens": 5}
        }"#;

        let items = parse_responses_api_output(json).unwrap();
        assert_eq!(items.len(), 1);
        match &items[0] {
            crate::ai::client::ResponsesOutputItem::Message { content } => {
                assert_eq!(content.len(), 1);
                match &content[0] {
                    crate::ai::client::ResponsesContentBlock::OutputText { text } => {
                        assert_eq!(text, "Hello!");
                    }
                }
            }
            _ => panic!("Expected Message variant"),
        }
    }

    #[test]
    fn test_parse_responses_api_function_call() {
        use crate::ai::client::parse_responses_api_output;

        let json = r#"{
            "output": [
                {
                    "type": "function_call",
                    "id": "call_123456",
                    "name": "get_weather",
                    "arguments": "{\"city\": \"Berlin\"}"
                }
            ]
        }"#;

        let items = parse_responses_api_output(json).unwrap();
        assert_eq!(items.len(), 1);
        match &items[0] {
            crate::ai::client::ResponsesOutputItem::FunctionCall { item_id, call_id, name, .. } => {
                assert_eq!(item_id, "call_123456");
                assert_eq!(call_id, "call_123456");
                assert_eq!(name, "get_weather");
            }
            _ => panic!("Expected FunctionCall variant"),
        }
    }

    #[test]
    fn test_parse_responses_api_reasoning() {
        use crate::ai::client::parse_responses_api_output;

        let json = r#"{
            "output": [
                {
                    "type": "reasoning",
                    "id": "rs_123",
                    "summary": [
                        {"type": "summary_text", "text": "I am thinking about weather."}
                    ],
                    "encrypted_content": "ENCRYPTED_THOUGHTS"
                },
                {
                    "type": "message",
                    "role": "assistant",
                    "content": [{"type": "output_text", "text": "It is sunny."}]
                }
            ]
        }"#;

        let items = parse_responses_api_output(json).unwrap();
        assert_eq!(items.len(), 2);
        
        match &items[0] {
            crate::ai::client::ResponsesOutputItem::Reasoning { summary, encrypted_content } => {
                assert_eq!(summary, "I am thinking about weather.");
                assert_eq!(encrypted_content, "ENCRYPTED_THOUGHTS");
            }
            _ => panic!("Expected Reasoning variant"),
        }
    }

    #[test]
    fn test_convert_reasoning_to_input() {
        use crate::ai::client::{Message, convert_messages_to_responses_input};

        let msg = Message {
            role: "assistant".to_string(),
            content: None,
            tool_calls: None,
            tool_call_id: None,
            reasoning_text: Some("Thinking...".to_string()),
            encrypted_reasoning: Some("SECRET".to_string()),
        };

        let (_, items) = convert_messages_to_responses_input(&[msg]);
        // Should have 2 items: Reasoning + Message
        assert_eq!(items.len(), 2);
        
        match &items[0] {
            crate::ai::client::ResponsesInputItem::Reasoning { encrypted_content, .. } => {
                assert_eq!(encrypted_content, "SECRET");
            }
            _ => panic!("First item should be Reasoning"),
        }
    }
}
