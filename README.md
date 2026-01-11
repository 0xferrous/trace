# trace - foundry trace explorer

An interactive, multi-platform trace explorer for [Foundry](https://github.com/foundry-rs/foundry)
that runs everywhere: your terminal, your browser, and as an API service.

## Why?

Foundry's traces are essential for debugging reverts, understanding call trees, and analyzing
transaction flow. However, large traces are difficult to navigate in a static format. This project
provides an interactive solution with the same powerful interface across all platforms.

## ✨ Features

- **Interactive Navigation**: Explore traces with keyboard-driven navigation, expand/collapse call
  trees, and quickly jump to relevant sections
- **Multi-Platform**: Same experience in your terminal (TUI) and web browser - powered by shared
  [ratatui](https://github.com/ratatui/ratatui) code
- **Seamless Integration**: Pipe directly from `cast run` - no intermediate files needed
- **Zero Installation Web Version**: Try it instantly at [0xf.rs/trace](https://0xf.rs/trace)

## 🎬 Demo

**Live Demo**: [https://0xf.rs/trace](https://0xf.rs/trace)

<!-- TODO: Add VHS recording GIF here -->
<!-- Record demo with: vhs demo.tape -->

## 📦 Installation

### Using Nix (Recommended)

```bash
# Run directly without installing
cast run <tx-hash> --json --rpc-url <rpc> | nix run github:0xferrous/traces-tui

# Or install to your profile
nix profile install github:0xferrous/traces-tui
```

### Using Cargo

```bash
cargo install --git https://github.com/0xferrous/traces-tui trace-cli
```

## 🚀 Usage

### Terminal (TUI)

Pipe trace data directly from `cast run`:

```bash
cast run <tx-hash> --json --rpc-url <rpc-url> | trace-cli tui
```

Or read from a file:

```bash
trace-cli tui <path/to/trace.json>
```

### Web

Visit [https://0xf.rs/trace](https://0xf.rs/trace) or run locally:

```bash
# Development server with hot reload
just serve-web

# Build for production
just build-web
```

The web version runs entirely in your browser using WebAssembly.

### Backend API

The backend provides a trace fetching service for the web interface:

```bash
# Using Nix
nix run .#backend -- --port 3000 --rpc-url <rpc-url-1> --rpc-url <rpc-url-2>

# Using Cargo
cargo run -p trace-backend -- --port 3000 --rpc-url <rpc-url>
```

## 🏗️ Architecture

This project demonstrates a powerful pattern: **write once, run everywhere** with Rust and
[ratatui](https://github.com/ratatui/ratatui).

```
┌─────────────────────────────────────────┐
│         trace-tui (core library)        │
│         Ratatui-based UI logic          │
└──────────────┬──────────────────────────┘
               │
       ┌───────┴────────┐
       │                │
┌──────▼─────┐   ┌──────▼──────┐
│  Terminal  │   │     Web     │
│  (CLI app) │   │    (WASM)   │
│            │   │             │
│ crossterm  │   │  ratzilla   │
│  backend   │   │  backend    │
└────────────┘   └─────────────┘
```

### Code Sharing Magic

- **Single UI Codebase**: The same ratatui widgets and layout code runs in both terminal and
  browser
- **Backend Abstraction**: Terminal uses `crossterm`, web uses `ratzilla` (WebAssembly-compatible
  backend)
- **Zero Duplication**: Features added to the UI automatically work on both platforms

### Crate Structure

- **`trace-tui`**: Core library with ratatui UI components (backend-agnostic)
- **`trace-cli`**: Terminal application using crossterm backend
- **`trace-web`**: WebAssembly application using ratzilla backend
- **`trace-backend`**: Axum-based API server for fetching traces from RPC endpoints

## 🐳 Docker

Docker images are available for both CLI and backend:

```bash
# Build images
just docker-build

# Run CLI
docker run -i ghcr.io/0xferrous/trace-cli tui < trace.json

# Run backend
docker run -p 3000:3000 ghcr.io/0xferrous/trace-backend \
  --port 3000 --rpc-url <rpc-url>
```

## 🛠️ Development

```bash
# Enter development shell (includes Rust, trunk, just, etc.)
nix develop

# Run TUI locally
cargo run -p trace-cli -- tui <trace-file>

# Run web dev server
just serve-web

# Run backend
cargo run -p trace-backend -- --rpc-url <rpc-url>
```

## 📄 License

MIT

## 🙏 Acknowledgments

Built with:
- [ratatui](https://github.com/ratatui/ratatui) - Terminal UI framework
- [ratzilla](https://github.com/orhun/ratzilla) - WebAssembly backend for ratatui
