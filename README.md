# avocode

An AI coding agent written in Rust. A port of [OpenCode](https://github.com/anomalyco/opencode).

Supports TUI, CLI, HTTP API, and 15+ AI providers out of the box. GitHub Copilot and OpenAI Codex work via OAuth with your existing subscription — no API key needed.

## Installation

**Homebrew (macOS/Linux):**

```bash
brew install smartcrabai/tap/avocode
```

**Shell script (all platforms):**

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/smartcrabai/avocode/releases/latest/download/avocode-installer.sh | sh
```

**Build from source:**

```bash
git clone https://github.com/smartcrabai/avocode
cd avocode
cargo build --release
./target/release/avocode
```

## Usage

```
avocode [OPTIONS] [COMMAND]

Commands:
  run        Start an interactive session (default)
  serve      Start the HTTP API server
  providers  List available providers
  models     List available models
  session    Manage sessions
  mcp        Manage MCP servers
  export     Export a session

Options:
  -m, --message <MESSAGE>  Run non-interactively with this prompt
  -s, --session <SESSION>  Resume an existing session by ID
      --model <MODEL>      Model to use (e.g. anthropic/claude-opus-4-5)
  -h, --help               Print help
  -V, --version            Print version
```

### TUI mode (default)

```bash
avocode
```

| Key | Action |
|-----|--------|
| `Enter` | Send message |
| `Ctrl+B` | Toggle sidebar |
| `Ctrl+T` | Cycle theme |
| `PageUp / PageDown` | Scroll chat |
| `Ctrl+C` | Quit |

Themes: Catppuccin Mocha · Dracula · Nord · Gruvbox · Tokyo Night

### Non-interactive mode

```bash
avocode --message "Refactor this function" --no-tui
avocode --model openai/gpt-4o --message "Write tests for this file"
```

### HTTP server mode

```bash
avocode serve --port 3000
```

Exposes a REST API and SSE event stream for sessions, providers, and configuration.

## Providers

| Provider | Auth | Env var |
|----------|------|---------|
| Anthropic | API key | `ANTHROPIC_API_KEY` |
| OpenAI | API key | `OPENAI_API_KEY` |
| Google Gemini | API key | `GOOGLE_API_KEY` |
| xAI (Grok) | API key | `XAI_API_KEY` |
| Mistral | API key | `MISTRAL_API_KEY` |
| Groq | API key | `GROQ_API_KEY` |
| Azure OpenAI | API key | `AZURE_OPENAI_API_KEY` |
| OpenRouter | API key | `OPENROUTER_API_KEY` |
| Cohere | API key | `COHERE_API_KEY` |
| Perplexity | API key | `PERPLEXITY_API_KEY` |
| Cerebras | API key | `CEREBRAS_API_KEY` |
| Together AI | API key | `TOGETHER_API_KEY` |
| DeepInfra | API key | `DEEPINFRA_API_KEY` |
| **GitHub Copilot** | OAuth (device flow) | subscription |
| **OpenAI Codex** | OAuth (PKCE / device flow) | subscription |

### GitHub Copilot

No API key required. Uses your existing Copilot subscription via GitHub OAuth device flow (RFC 8628). The GitHub token is exchanged for a short-lived Copilot session token, which is refreshed automatically 5 minutes before expiry.

### OpenAI Codex

Uses your ChatGPT Codex subscription. Choose between a headless device code flow or a browser-based PKCE flow (localhost:1455 callback).

## Configuration

Config files use JSON with Comments (JSONC). Three layers are merged in order:

1. System: `/etc/avocode/config.jsonc`
2. User: `~/.config/avocode/config.jsonc`
3. Project: `.avocode/config.jsonc`

```jsonc
{
  "model": "anthropic/claude-opus-4-5",

  "providers": {
    "anthropic": { "apiKey": "sk-ant-..." }
  },

  // MCP servers
  "mcp": {
    "servers": {
      "filesystem": {
        "command": "npx",
        "args": ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
      }
    }
  },

  // allow / deny / ask
  "permissions": {
    "bash": "ask",
    "write": "ask",
    "read": "allow"
  }
}
```

## Architecture

```
src/
├── main.rs          Entry point — delegates to cli::run()
├── types.rs         Newtype wrappers: SessionId, MessageId, ProjectId
├── app.rs           AppContext — Arc-shared runtime state
│
├── cli/             clap subcommands
├── tui/             ratatui TUI (5 themes, chat/input/sidebar)
├── server/          axum REST API + SSE event stream
│
├── llm/             Streaming LLM clients
│   ├── openai.rs    OpenAI / Azure / Codex
│   ├── anthropic.rs Anthropic Claude
│   ├── google.rs    Google Gemini
│   └── sse.rs       SSE parser
│
├── provider/        Model registry with models.dev 24h cache
├── auth/            OAuth credential store (Copilot / Codex)
├── session/         Session & message management (SQLite)
├── storage/         SQLite schema & migrations
├── tool/            bash / read / write / edit / glob / grep / ls / webfetch
├── permission/      Wildcard rule evaluation & async manager
├── mcp/             MCP client — JSON-RPC 2.0 over stdio / SSE / HTTP
├── agent/           Built-in agents & system prompt assembly
├── config/          JSONC loader with multi-layer merge
├── event/           tokio broadcast event bus
└── error/           AppError, NamedError trait
```

## Development

```bash
cargo build
cargo test          # 195 tests
cargo clippy --all-features
```
