# OBS Remote Volume Meter

A lightweight native desktop monitor for OBS audio levels. It connects directly to OBS WebSocket 5.x and displays OBS-style segmented volume meters without requiring a browser runtime.

## Why this exists

OBS provides audio meters inside its own interface, but they are not always convenient to keep visible while working in other applications. OBS Remote Volume Meter provides a focused, detachable view for monitoring one or more OBS connections.

## Features

- Live segmented meters with level markings, decay, peak hold, and clipping indication.
- Multiple simultaneous OBS WebSocket connections and audio sources.
- Saved connection profiles, including optional automatic startup and reconnect behavior.
- Light and dark themes, horizontal and vertical layouts, and an enlarged Large mode (`F11`).
- Native binaries for Linux, macOS, and Windows.

## Quick start

1. In OBS, open **Tools → WebSocket Server Settings**, enable the server, and note its port and password. The default port is `4455`.
2. Install Rust 1.85 or newer.
3. Run the application:

   ```sh
   cargo run --release
   ```

4. Open **Connections** in the app, enter the OBS connection details, and connect.

## Installation and releases

Build an optimized binary locally with:

```sh
cargo build --release
```

The executable is written to `target/release/`. GitHub Actions builds Linux, macOS, and Windows binaries when `src/`, `Cargo.toml`, or `Cargo.lock` changes on `master`. Push a `v*` tag, such as `v1.1.0`, to automatically create a GitHub Release with all three binaries attached. The macOS release is a DMG containing a standard app bundle and an Applications shortcut.

On Linux, the build may require the usual X11/Wayland and OpenGL development packages used by `eframe`.

## Configuration

Connection profiles are stored in `connection.json` beside the executable. The file contains connection passwords as plain text, so protect it appropriately. The app can start connections automatically, retry unavailable or lost connections, switch themes and orientation, and enter Large mode with `F11`.

## Documentation

- [Contributing guide](CONTRIBUTING.md)
- [OBS WebSocket documentation](https://github.com/obsproject/obs-websocket)
- [GitHub Actions build workflow](.github/workflows/build-binaries.yml)

## License

This project is licensed under the [GNU General Public License v3.0](https://www.gnu.org/licenses/gpl-3.0.en.html).
