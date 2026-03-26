// Models previously used for typed Anthropic/OpenAI deserialization.
// Now the proxy uses raw `serde_json::Value` for passthrough to avoid
// "Input tag 'Other'" errors with unknown content block types.
// This module is intentionally kept minimal.
