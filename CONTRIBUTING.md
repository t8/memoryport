# Contributing to Memoryport

Thanks for your interest in contributing to Memoryport! This document covers the basics of how to build, test, and submit changes.

## Getting Started

### Prerequisites

- Rust stable (1.91+)
- `protoc` (Protocol Buffers compiler) — `brew install protobuf` on macOS
- Node.js 18+ and pnpm (for the UI)
- Ollama (optional, for local embeddings)

### Building

```bash
# Clone the repo
git clone https://github.com/t8/memoryport.git
cd memoryport

# Build everything
cargo build

# Run tests
cargo test --workspace

# Build + run the UI
cd ui && pnpm install && pnpm dev
```

### Local Development

Use the dev script to manage all services:

```bash
./dev.sh start    # Build and start server, proxy, UI
./dev.sh stop     # Stop all services
./dev.sh restart  # Rebuild and restart everything
./dev.sh status   # Show what's running
./dev.sh logs proxy  # Tail proxy logs
```

## Architecture

See [CLAUDE.md](./CLAUDE.md) for detailed architecture notes, conventions, and technical decisions.

The workspace has 9 crates:

| Crate | Purpose |
|-------|---------|
| `uc-arweave` | Arweave client (JWK wallet, ANS-104, Turbo uploads) |
| `uc-embeddings` | Embedding + LLM providers (OpenAI, Ollama) |
| `uc-core` | Core engine: chunker, indexer, retriever, writer, gating |
| `uc-cli` | CLI binary with setup wizard |
| `uc-mcp` | MCP server for Claude Code / Cursor |
| `uc-proxy` | Multi-protocol API proxy (Anthropic, OpenAI, Ollama) |
| `uc-server` | Multi-tenant HTTP API server |
| `uc-tauri` | Tauri desktop app |

Frontend: `ui/` — React 19 + Vite + Tailwind.

## Code Style

- **Errors**: `thiserror` derive macros, one error enum per module
- **Async**: `tokio` everywhere
- **Logging**: `tracing` crate (never `println!` or `eprintln!` in library/server code)
- **Config**: TOML files parsed with `serde` + `toml`
- **Tests**: unit tests in `#[cfg(test)]` modules, integration tests in `tests/`

## Submitting Changes

1. Fork the repo and create a branch from `main`
2. Make your changes
3. Run `cargo test --workspace` and `cargo clippy --workspace`
4. Submit a pull request against `main`

Keep PRs focused — one feature or fix per PR. Include a clear description of what changed and why.

## Reporting Issues

File issues at [github.com/t8/memoryport/issues](https://github.com/t8/memoryport/issues). Include:
- What you expected to happen
- What actually happened
- Steps to reproduce
- Your OS and Rust version (`rustc --version`)

## License

By contributing, you agree that your contributions will be licensed under the [Apache-2.0 License](./LICENSE).
