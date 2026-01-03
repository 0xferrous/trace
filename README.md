# traces-tui

This project addresses the long-standing lack of of interactive traces explorer in [foundry](https://github.com/foundry-rs/foundry).

## Why?

Traces are a great way to debug why something reverted and in what step, what the call tree
looks like, etc. However, large traces are hard to read and navigate. A TUI offers an ergonomic
way around that.

## Running with nix

```bash
# pipe the trace to the TUI
cast run <tx> --json --rpc-url <rpc> | nix run github:0xferrous/traces-tui tui
# read the trace from a file
nix run github:0xferrous/traces-tui tui <path/to/trace.json>
```

## Installing with cargo and running

```bash
# install the tui
cargo install --git https://github.com/0xferrous/traces-tui traces-cli 
# invoking the tui
traces-cli tui <path/to/trace.json>
# or pipe the trace to the tui
cast run <tx> --json --rpc-url <rpc> | traces-cli tui
```

