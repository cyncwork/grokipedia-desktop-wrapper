# Contributing to Grokipedia Desktop

Thanks for your interest! This is a solo-maintained project, so here are some guidelines to keep things manageable.

## Before You Start

- **Open an issue first.** Before submitting a PR, open an issue describing what you want to change and why. This avoids wasted effort if the change doesn't fit the project direction.
- **Bug reports are always welcome.** Include your OS, steps to reproduce, and what you expected vs. what happened.

## Pull Requests

- Keep PRs focused — one change per PR.
- Make sure the project compiles before submitting:
  ```bash
  source "$HOME/.cargo/env" && cargo build --manifest-path src-tauri/Cargo.toml
  ```
- Don't include unrelated formatting changes or refactors.
- Describe what the PR does and how to test it.

## What's Welcome

- Bug fixes
- Linux compatibility improvements
- Performance improvements
- Documentation fixes

## What's Not In Scope

- Adding support for sites other than grokipedia.com
- Major architectural changes without prior discussion
- Adding build tools, bundlers, or frameworks to the UI layer

## Code Style

- Rust: standard `rustfmt` conventions
- JS/CSS: vanilla, no frameworks, no bundler — keep it simple
