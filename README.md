# OBS Remote Volume Meter

This app provides a way to monitor OBS volume meters remotely so you can check your levels on another computer.

## Native application

The lightweight Rust/egui implementation is in [`native/`](native/README.md). It connects directly to OBS WebSocket without Electron, Node.js, or a browser engine at runtime.

The original React prototype remains in `src/`.
