# tiro

A TUI note-taking app with tag filtering and an optional embedded LLM agent.

## Build

Requires a recent Rust toolchain (edition 2024).

```sh
cargo build --release
```

The binary will be in `target/release/tiro`. Run it with `cargo run --release`
or copy it onto your `PATH`.

Notes live in `~/.local/tiro/notes` and agent sessions in
`~/.local/tiro/sessions`. Both directories are created on first run.

## Configuration

Config is read from `~/.local/tiro/config.toml`. It is optional -- if absent,
tiro starts with a built-in tag set and AI disabled.

Paths inside the config (`api_key_path`, `system_prompt_path`) are resolved
relative to the config file's directory.

### Example `~/.local/tiro/config.toml`

```toml
# Custom tags for the notes.
# If absent, the colour will be obtained from a checksum on the name.
[[tag]]
name = "work"
# use CSS-style hexdec to define the colour
color = "#4f9dff"

[[tag]]
name = "personal"
color = "#ffb86c"

[[tag]]
name = "idea"

# Optional AI agent features. Omit entirely to disable AI.
[agent]
# Currently supported providers:
# "openai" | "anthropic" | "ollama" | "none"
provider = "anthropic"          
model = "claude-haiku-4-5"
# (resolved relative to config file:)
api_key_path = "secrets/anthropic.key"

# api_key = "sk-..."  # <- probs safer not to do this

# omit for default prompt:
system_prompt_path = "prompts/agent.md"
# for ollama rather than external providers:
ollama_url = "http://localhost:11434"
```
