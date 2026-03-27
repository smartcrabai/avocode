# avocode

Rust製AIコーディングエージェント。[OpenCode](https://github.com/anomalyco/opencode) のRustポート。

TUI・CLI・HTTP API・20以上のAIプロバイダーに対応。GitHub CopilotとOpenAI CodexはサブスクリプションのままOAuth認証で利用可能。

## インストール

```bash
cargo install --git https://github.com/smartcrabai/avocode
```

または手元でビルド：

```bash
git clone https://github.com/smartcrabai/avocode
cd avocode
cargo build --release
./target/release/avocode
```

## 使い方

```
avocode [OPTIONS] [COMMAND]

Commands:
  run        インタラクティブセッションを開始（デフォルト）
  serve      HTTP APIサーバーを起動
  providers  利用可能なプロバイダーを一覧表示
  models     利用可能なモデルを一覧表示
  session    セッション管理
  mcp        MCPサーバー管理
  export     セッションをエクスポート

Options:
  -m, --message <MESSAGE>  非インタラクティブモードで実行するプロンプト
  -s, --session <SESSION>  継続するセッションID
      --model <MODEL>      使用するモデル（例: anthropic/claude-opus-4-5）
  -h, --help               ヘルプを表示
  -V, --version            バージョンを表示
```

### TUIモード（デフォルト）

```bash
avocode
```

| キー | 動作 |
|------|------|
| `Enter` | メッセージ送信 |
| `Ctrl+B` | サイドバー表示切替 |
| `Ctrl+T` | テーマ切替（5種） |
| `PageUp / PageDown` | チャットスクロール |
| `Ctrl+C` | 終了 |

テーマ: Catppuccin Mocha / Dracula / Nord / Gruvbox / Tokyo Night

### 非インタラクティブモード

```bash
avocode --message "このコードをリファクタリングして" --no-tui
avocode --model openai/gpt-4o --message "テストを書いて"
```

### HTTPサーバーモード

```bash
avocode serve --port 3000
```

REST API + SSEイベントストリームでセッション・プロバイダー・設定を操作可能。

## 対応プロバイダー

| プロバイダー | 認証方式 | 環境変数 |
|-------------|----------|---------|
| Anthropic | API Key | `ANTHROPIC_API_KEY` |
| OpenAI | API Key | `OPENAI_API_KEY` |
| Google (Gemini) | API Key | `GOOGLE_API_KEY` |
| xAI (Grok) | API Key | `XAI_API_KEY` |
| Mistral | API Key | `MISTRAL_API_KEY` |
| Groq | API Key | `GROQ_API_KEY` |
| Azure OpenAI | API Key | `AZURE_OPENAI_API_KEY` |
| OpenRouter | API Key | `OPENROUTER_API_KEY` |
| Cohere | API Key | `COHERE_API_KEY` |
| Perplexity | API Key | `PERPLEXITY_API_KEY` |
| Cerebras | API Key | `CEREBRAS_API_KEY` |
| Together AI | API Key | `TOGETHER_API_KEY` |
| DeepInfra | API Key | `DEEPINFRA_API_KEY` |
| **GitHub Copilot** | デバイスフロー OAuth | サブスクリプション認証 |
| **OpenAI Codex** | PKCE / デバイスフロー OAuth | サブスクリプション認証 |

### GitHub Copilot 認証

APIキー不要。GitHubアカウントのCopilotサブスクリプションをそのまま使用します。

RFC 8628 デバイスフローでGitHubにOAuth認証し、取得したGitHubトークンをCopilot内部APIでセッショントークンに交換します（5分マージンで自動リフレッシュ）。

### OpenAI Codex 認証

ChatGPTのCodexサブスクリプションを使用します。ブラウザ不要のデバイスコードフローまたはPKCEフロー（ポート1455）を選択できます。

## 設定

設定ファイルはJSON with Comments (JSONC) 形式。複数レイヤーをマージして適用します：

1. システム設定: `/etc/avocode/config.jsonc`
2. ユーザー設定: `~/.config/avocode/config.jsonc`
3. プロジェクト設定: `.avocode/config.jsonc`

```jsonc
{
  // デフォルトモデル
  "model": "anthropic/claude-opus-4-5",

  // プロバイダー別設定
  "providers": {
    "anthropic": {
      "apiKey": "sk-ant-..."
    }
  },

  // MCPサーバー設定
  "mcp": {
    "servers": {
      "filesystem": {
        "command": "npx",
        "args": ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
      }
    }
  },

  // 権限設定（allow / deny / ask）
  "permissions": {
    "bash": "ask",
    "write": "ask",
    "read": "allow"
  }
}
```

## アーキテクチャ

```
src/
├── main.rs          — エントリポイント（cli::run() に委譲）
├── types.rs         — SessionId / MessageId 等の新型ラッパー
├── app.rs           — AppContext（Arc共有ランタイム状態）
│
├── cli/             — clap CLIサブコマンド
├── tui/             — ratatui TUI（5テーマ、chat/input/sidebar）
├── server/          — axum REST API + SSEイベントストリーム
│
├── llm/             — LLMストリーミングクライアント
│   ├── openai.rs    — OpenAI / Azure / Codex
│   ├── anthropic.rs — Anthropic Claude
│   ├── google.rs    — Google Gemini
│   └── sse.rs       — SSEパーサー
│
├── provider/        — モデルレジストリ（models.dev 24hキャッシュ）
├── auth/            — OAuth認証ストア（Copilot / Codex）
├── session/         — セッション・メッセージ管理（SQLite）
├── storage/         — SQLiteスキーマ・マイグレーション
├── tool/            — ツール実装（bash/read/write/edit/glob/grep/ls/webfetch）
├── permission/      — 権限評価（ワイルドカードルール）
├── mcp/             — MCPクライアント（JSON-RPC 2.0 / stdio / SSE）
├── agent/           — エージェント定義・システムプロンプト
├── config/          — JSONC設定ローダー（多階層マージ）
├── event/           — tokio broadcastイベントバス
└── error/           — AppError / NamedError trait
```

## ビルド・テスト

```bash
# ビルド
cargo build

# テスト（195件）
cargo test

# Clippyチェック
cargo clippy --all-features

# リリースビルド
cargo build --release
```

## 動作要件

- Rust 2024 edition (rustup推奨)
- macOS / Linux / Windows

## ライセンス

MIT
