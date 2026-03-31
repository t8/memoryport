# Memoryport Settings and Integration Modes

This document covers all configuration options, integration modes, and how to choose the right setup for your workflow.

## Integration Modes

Memoryport offers two complementary integration methods: the **MCP Server** and the **API Proxy**. You can use one or both depending on your tools and preferences.

### MCP Server

The MCP (Model Context Protocol) server gives your editor direct access to memory tools via the stdio transport. It starts automatically when your editor launches -- there is no background process to manage.

**Who should use it:** Anyone using Claude Code, Claude Desktop, Cursor, or another MCP-compatible client.

**How it works:** The editor spawns `uc-mcp` as a child process over stdio. The server exposes 7 tools and 2 resources:

| Tool | Description |
|------|-------------|
| `uc_auto_store` | Store a conversation turn to persistent memory. Called automatically by the LLM on each user/assistant message. |
| `uc_store` | Save information with explicit metadata (chunk type, role, session). |
| `uc_query` | Semantic search with full retrieval pipeline. Returns assembled context. |
| `uc_retrieve` | Raw ranked results with scores and metadata. |
| `uc_get_chunk` | Look up a specific chunk by ID (for `memoryport://chunk/` references). |
| `uc_get_session` | Full transcript of a specific conversation session. |
| `uc_list_sessions` | List all stored sessions with dates and sizes. |
| `uc_status` | System status: indexed chunks, embedding model, pending writes. |

| Resource | Description |
|----------|-------------|
| `uc://context/auto` | Recent context from the active session. |
| `uc://sessions/{id}` | Full conversation history for a session (template). |

**How to register:** Run `uc init` and the wizard registers the MCP server in your editor config automatically. Manual registration:

```json
// ~/.claude.json (Claude Code)
{
  "mcpServers": {
    "memoryport": {
      "command": "/path/to/uc-mcp",
      "args": ["--config", "/Users/you/.memoryport/uc.toml"]
    }
  }
}
```

```json
// ~/.cursor/mcp.json (Cursor)
{
  "mcpServers": {
    "memoryport": {
      "command": "/path/to/uc-mcp",
      "args": ["--config", "/Users/you/.memoryport/uc.toml"]
    }
  }
}
```

**Key behavior:** MCP queries bypass retrieval gating because they are explicit user requests (the LLM decided to call the tool). The server uses `search()` directly rather than the gated `query()` pipeline.

### API Proxy

The API proxy sits between your editor and the AI provider, transparently intercepting every request and response. It captures full conversations (both user messages and assistant responses) and injects relevant context from memory.

**Who should use it:** Anyone who wants automatic, zero-config conversation capture. Particularly useful for tools that do not support MCP.

**How it works:** The proxy listens on a single port (default `127.0.0.1:9191`) and handles three API formats:

| Protocol | Endpoint | Used by |
|----------|----------|---------|
| Anthropic | `/v1/messages` | Claude Code, Claude API clients |
| OpenAI | `/v1/chat/completions` | Cursor, ChatGPT API clients, any OpenAI SDK app |
| Ollama | `/api/chat`, `/api/generate`, `/api/tags`, `/api/*` | Open WebUI, Continue.dev, Ollama CLI |

For each request, the proxy:

1. Extracts the last user message
2. Searches memory for relevant context (unless gated or in capture-only mode)
3. Injects context by appending it to the last user message as plain text
4. Forwards the request to the real upstream (Anthropic, OpenAI, or Ollama on localhost:11434)
5. Captures the user message and assistant response, sanitizes them, and stores them in the index
6. Returns the response to the client

The proxy auto-detects the upstream based on the model name in the request: models starting with `gpt-`, `o1`, `o3` route to OpenAI; models containing `:` or starting with `llama`, `mistral`, `gemma`, `phi`, `qwen`, `deepseek` route to Ollama on localhost:11434; everything else defaults to OpenAI.

**Setup for common tools:**

| Tool | Configuration |
|------|---------------|
| Claude Code | `uc init` sets `ANTHROPIC_BASE_URL=http://127.0.0.1:9191` in `~/.claude.json` |
| Cursor | Set `ANTHROPIC_BASE_URL=http://127.0.0.1:9191` in your environment |
| Open WebUI | Settings > Connections > Ollama URL: `http://127.0.0.1:9191` |
| Ollama CLI | `OLLAMA_HOST=http://127.0.0.1:9191 ollama run llama3` |
| Continue.dev | Set endpoint to `http://127.0.0.1:9191` |
| Any OpenAI SDK app | `OPENAI_BASE_URL=http://127.0.0.1:9191` |

### Using Both MCP and Proxy Together

When both MCP and proxy are active, the proxy automatically switches to **capture-only mode** to avoid duplicate context injection. The MCP server handles retrieval and injection, while the proxy silently records all conversations.

This is detected automatically: the proxy checks `~/.claude.json` for a `mcpServers.memoryport` entry. If found, the proxy skips context injection but still captures both sides of every conversation.

You can see this reflected in the dashboard Settings page, where the "Memorize only" toggle auto-enables when both integrations are active, with the explanation: "MCP and proxy are both active. The proxy records conversations while MCP handles context injection, avoiding duplicate injection that can corrupt API requests."

### When to Use Which

| Scenario | Recommendation |
|----------|---------------|
| Claude Code only | MCP alone is sufficient. The LLM calls memory tools as needed. |
| Claude Code + full capture | Both. MCP handles injection, proxy captures everything. |
| Cursor | Proxy. Set `ANTHROPIC_BASE_URL` to the proxy address. Add MCP via `~/.cursor/mcp.json` for tool access. |
| Open WebUI / Ollama | Proxy only. MCP is not supported by these tools. |
| Multiple tools simultaneously | Both. MCP for tools that support it, proxy for transparent capture across all tools. |
| Capture without injection | Proxy in capture-only mode. Set `capture_only = true` in config. |

## Embedding Provider

Embeddings convert text into vector representations for semantic search. This is the core engine behind memory retrieval.

### OpenAI

Cloud-hosted embeddings. Fast, high quality, requires an API key and internet connection.

```toml
[embeddings]
provider = "openai"
model = "text-embedding-3-small"
dimensions = 1536
api_key = "sk-..."  # or set OPENAI_API_KEY env var
```

- **Model**: `text-embedding-3-small` is the default. You can use `text-embedding-3-large` (3072 dimensions) for higher quality at higher cost.
- **API key**: Set in the config file or via the `OPENAI_API_KEY` environment variable. The config file value takes precedence.
- **Custom endpoint**: Use `api_base` for OpenAI-compatible providers:

```toml
[embeddings]
provider = "openai"
model = "custom-model"
dimensions = 1024
api_base = "https://your-provider.example.com/v1"
```

### Ollama

Local embeddings. Free, private, no internet required. Slower than OpenAI but your data never leaves your machine.

```toml
[embeddings]
provider = "ollama"
model = "nomic-embed-text"
dimensions = 768
```

- **Model**: `nomic-embed-text` is the default (768 dimensions). Other options include `mxbai-embed-large` (1024d) or any Ollama embedding model.
- **API key**: Not required.
- **Custom host**: If Ollama runs on a non-default address, set `api_base`:

```toml
[embeddings]
provider = "ollama"
model = "nomic-embed-text"
dimensions = 768
api_base = "http://192.168.1.100:11434"
```

### Changing Providers

Changing the embedding model makes all existing vectors incompatible. The dashboard warns: "All stored vectors were computed with [old provider/model]. Switching models means new embeddings will be incompatible with old ones."

To switch providers:

1. Update `[embeddings]` in your config
2. Update `[index] embedding_dimensions` to match the new model
3. Delete the local index: `rm -rf ~/.memoryport/index`
4. If you have Arweave backup, rebuild: `uc rebuild-index -u default`
5. If local-only, you start fresh

## Retrieval Settings

### Smart Gating

Three-gate system that prevents unnecessary retrieval on trivial messages.

```toml
[retrieval]
gating_enabled = true          # default: true
min_relevance_score = 0.3      # default: 0.3
```

**What it does:**

- **Gate 1 (Rules)**: Skips retrieval for greetings ("hi", "thanks"), short commands, and trivial queries. Forces retrieval for memory references ("do you remember...") and temporal queries.
- **Gate 2 (Embedding routing)**: Compares the query embedding against learned centroids to determine if the query likely needs memory context. Reuses the embedding already computed for search, so it adds zero latency.
- **Gate 3 (Quality threshold)**: Drops results with a relevance score below `min_relevance_score`. Filters out low-quality matches after retrieval.

**When to disable:** If you want every single request to trigger a memory search, set `gating_enabled = false`. This increases latency on trivial messages but ensures nothing is missed.

### Multi-turn / Agentic Retrieval

Instead of injecting context in a single shot, the proxy gives the LLM a memory search tool and lets it decide what to search for.

```toml
[proxy.agentic]
enabled = true                 # default: true
max_rounds = 3                 # default: 3
```

**What it does:** The proxy injects a tool definition into the request. The LLM can call this tool to search memory, inspect the results, and optionally search again with a refined query. After `max_rounds` tool calls (or when the LLM produces a final response), the proxy returns the result.

**Tradeoffs:**

- Higher quality retrieval: the LLM formulates its own search queries based on the conversation context.
- Higher latency: each round adds an extra LLM call.
- Only works via the proxy (not MCP, which has its own tool-based retrieval).
- Does not work with models that do not support tool calling. The proxy tracks which models fail tool calls and falls back to single-shot injection for those models.

**Single-turn fallback:** When agentic mode is disabled (or the model does not support tools), the proxy falls back to single-shot injection: it searches memory, assembles context, and appends it to the last user message before forwarding.

**Capture-only mode:** When `capture_only = true`, the proxy skips both injection modes entirely and only records conversations.

```toml
[proxy]
capture_only = false           # default: false
```

This is auto-enabled when both MCP and proxy are active. You can also set it manually if you want the proxy to record conversations without influencing responses.

### Other Retrieval Parameters

```toml
[retrieval]
max_context_tokens = 50000     # max tokens in assembled context
recency_window = 20            # recent chunks considered for recency scoring
similarity_top_k = 50          # candidates pulled from vector search before ranking
```

- `max_context_tokens`: Controls how much context is assembled from search results. Higher values give the LLM more context but use more of the model's context window.
- `recency_window`: Number of most recent chunks that get a recency boost in scoring. Helps surface recent conversations.
- `similarity_top_k`: How many candidates to pull from the vector index before reranking and filtering. Higher values improve recall but increase latency slightly.

### Advanced: Query Enhancement

These features use an LLM to improve search queries. They are disabled by default because they add latency and cost.

```toml
[retrieval]
query_expansion = true          # LLM generates alternative search terms
hyde = true                     # Hypothetical Document Embeddings
llm_provider = "openai"         # or "ollama"
llm_model = "gpt-4o-mini"
```

- **Query expansion**: The LLM generates multiple alternative phrasings of the search query, and all are searched in parallel. Improves recall for ambiguous queries.
- **HyDE (Hypothetical Document Embeddings)**: Instead of embedding the raw query, the LLM generates a hypothetical answer, and that answer is embedded. This can improve results when the query phrasing differs from the stored content phrasing.

## Arweave Storage

Arweave provides permanent, decentralized storage. Memories are uploaded as encrypted ANS-104 data items and stored forever with a single payment.

### Configuration

```toml
[arweave]
gateway = "https://arweave.net"
turbo_endpoint = "https://upload.ardrive.io"
wallet_path = "~/.memoryport/wallet.json"
api_key = "uc_your_key_here"
api_endpoint = "https://memoryport.ai/api"
enabled = true
```

### API Key

A Memoryport Pro subscription at [memoryport.ai](https://memoryport.ai) provides:

- A `uc_` API key for authentication
- Turbo credits for Arweave uploads (the cost is covered by your subscription)
- Usage tracking and billing management

Set the key in the config file or via the `UC_API_KEY` environment variable. The dashboard validates the key automatically when you enter it and shows your storage usage.

### Wallet

An Arweave JWK wallet is auto-generated the first time the engine starts with a valid API key. The wallet is stored at the path specified in `wallet_path` (default `~/.memoryport/wallet.json`).

The wallet is your signing key for Arweave transactions. To move to a new machine, export the wallet keyfile from Settings and import it on the new machine. Without the wallet, you cannot write new data to Arweave or prove ownership of existing data.

You can also import an existing wallet keyfile through the dashboard (Settings > Arweave Storage > Import keyfile) or by placing the JSON file at the configured `wallet_path`.

### Cost Model

Uploads use Turbo credits included with your Pro subscription. The credit sharing system works via an `x-paid-by` header so that your local wallet's uploads are billed to your subscription. You do not need to fund the Arweave wallet directly.

Storage usage is tracked per month and displayed in the dashboard Settings page as a progress bar showing tokens used vs. your plan limit.

### Sync and Rebuild

- **Sync to Arweave**: Uploads any local memories that have not been synced to permanent storage yet. Available in the dashboard Settings page.
- **Rebuild from Arweave**: Downloads all your encrypted batches from Arweave and re-indexes them locally. Use when setting up on a new machine. Requires your wallet keyfile and encryption passphrase.

## Encryption

All data uploaded to Arweave can be encrypted with AES-256-GCM. Local data in LanceDB is not encrypted.

### How It Works

- Each batch (up to 50 chunks) gets a unique random encryption key.
- That per-batch key is wrapped (encrypted) with a master key derived from your passphrase via Argon2id (with a 16+ byte salt).
- The wrapped key is stored alongside the batch. Only someone with the master passphrase can unwrap it.

### Configuration

```toml
[encryption]
enabled = true
passphrase = "your-secure-passphrase"
```

Or use an environment variable:

```toml
[encryption]
enabled = true
passphrase_env = "UC_MASTER_PASSPHRASE"
```

Then set the environment variable:

```bash
export UC_MASTER_PASSPHRASE="your-secure-passphrase"
```

The `passphrase_env` key defaults to `UC_MASTER_PASSPHRASE`. If both `passphrase` and `passphrase_env` are set, the direct `passphrase` value is used.

### Logical Deletion

Since Arweave data is permanent, you cannot delete the ciphertext. Instead, logical deletion destroys the per-batch encryption key. Without the key, the ciphertext is permanently unreadable:

```bash
uc delete --tx-id <arweave-transaction-id>
```

### Important Warnings

- **Do not lose your passphrase.** If you lose it, all encrypted data on Arweave becomes permanently inaccessible.
- **Do not disable encryption** if you have existing encrypted data. The dashboard warns: "Disabling encryption means previously encrypted data on Arweave will become permanently inaccessible during rebuild."
- **Passphrase is needed for rebuild.** When rebuilding from Arweave on a new machine, you need both the wallet keyfile and the encryption passphrase.

## Proxy Settings

### Listen Address

```toml
[proxy]
listen = "127.0.0.1:9191"
```

The proxy listens on this address. The `uc init` wizard sets this to `127.0.0.1:9191`. You can change the port if it conflicts with another service.

The `uc-proxy` binary also accepts a `--listen` flag that overrides the config value:

```bash
uc-proxy --config ~/.memoryport/uc.toml --listen 127.0.0.1:8888
```

### Upstream Override

```toml
[proxy]
upstream = "https://api.anthropic.com"
```

By default, the proxy routes Anthropic requests to `https://api.anthropic.com`. Set `upstream` to override this for custom endpoints or proxies.

For OpenAI-format requests, the upstream is auto-detected from the model name. For Ollama, requests always go to `http://127.0.0.1:11434`.

### Supported API Formats

All three formats are served on a single port:

| Format | Route | Upstream |
|--------|-------|----------|
| Anthropic | `POST /v1/messages` | Config `upstream` or `https://api.anthropic.com` |
| OpenAI | `POST /v1/chat/completions` | Auto-detected by model name |
| Ollama | `ANY /api/*` | `http://127.0.0.1:11434` |
| Health | `GET /health` | Returns `"ok"` |
| Ollama root | `GET /` | Returns `"Ollama is running"` (so Ollama clients detect it) |

### Hot-Reload Behavior

The proxy watches the config file's modification time on every request. When the file changes, it reloads `[proxy.agentic]` and `[proxy] capture_only` settings without restarting. This means you can toggle multi-turn retrieval or capture-only mode from the dashboard and have it take effect immediately.

Other settings (listen address, embedding provider, etc.) require a proxy restart to take effect.

### Session Management

The proxy maintains session IDs per source (e.g., `openai`, `ollama`). A new session is created after 30 minutes of inactivity. Session state is persisted to `~/.memoryport/proxy-sessions.json` so sessions survive proxy restarts.

Session IDs follow the format `{source}-{YYYYMMDD-HHMMSS}`, for example `openai-20250315-141020`.

### Content Sanitization

Before storing captured text, the proxy sanitizes it by removing:

- `<system-reminder>` tags and their contents
- `<local-command-*>` tags
- Memory file dumps from Claude Code
- Open WebUI meta-requests (title generation, tag generation, emoji generation) are detected and skipped entirely for both injection and capture

### Authentication Forwarding

The proxy forwards `Authorization` and `x-api-key` headers to the upstream. Your API keys are passed through transparently -- the proxy does not store or log them.
