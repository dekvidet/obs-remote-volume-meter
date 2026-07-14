# OBS Remote Volume Meter — native

A lightweight native replacement for the React prototype. It connects directly to OBS WebSocket 5.x and renders OBS-style segmented meters without embedding a browser engine.

## Requirements

- OBS Studio 28 or newer (OBS WebSocket is built in)
- Rust 1.85 or newer (the project uses the Rust 2024 edition)
- On Linux, the normal X11/Wayland and OpenGL development packages required by `eframe`

Enable the WebSocket server in **OBS → Tools → WebSocket Server Settings**. The default port is `4455`; keep authentication enabled.

## Run

```sh
cd native
cargo run --release
```

Use **Connections…** to add, edit, remove, and connect to saved OBS servers. Each connection stores its host, port, WebSocket password, and auto-connect preference in `connection.json` beside the executable. Passwords are stored in that file as plain text, so protect it accordingly. Existing single-connection files are migrated automatically.

Use **Settings…** to switch between light and dark themes or horizontal and vertical meters. **Large** mode (keyboard shortcut: `F11`) hides the controls and labels and displays enlarged meter bars; press `F11` again to leave it.

## Build

```sh
cargo build --release
```

Build on each target operating system for the simplest packaging.

## Implementation

- `src/obs.rs` implements the OBS WebSocket 5.x handshake, authentication, and meter subscription.
- `src/meter.rs` converts linear multipliers to dB and paints segmented channels, ticks, decay, peak hold, and clipping.
- `src/main.rs` contains the application and connection UI.
