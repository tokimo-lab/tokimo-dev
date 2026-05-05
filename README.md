# @tokimo-lab/port-ops

[![CI](https://github.com/tokimo-lab/tokimo-dev/actions/workflows/CI.yml/badge.svg)](https://github.com/tokimo-lab/tokimo-dev/actions/workflows/CI.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

Cross-platform native addon to find and kill processes by port number. Zero JS dependencies — pure Rust via [napi-rs](https://napi.rs).

## Install

```bash
pnpm add @tokimo-lab/port-ops
```

From git (requires Rust toolchain):

```bash
pnpm add github:tokimo-lab/tokimo-dev
```

## API

```ts
import { findPidByPort, kill, killByPort } from '@tokimo-lab/port-ops'

// Find which process owns port 3000
const pid = findPidByPort(3000) // number | null

// Kill a process (SIGTERM / graceful)
kill(pid)

// Force kill (SIGKILL / TerminateProcess)
kill(pid, true)

// Find + kill in one call
const killed = killByPort(3000)       // number | null
const killed = killByPort(3000, true) // force
```

### `findPidByPort(port: number): number | null`

Returns the PID of the process listening on `port`, or `null` if no process is found. Searches TCP (LISTEN state) and UDP on both IPv4 and IPv6.

### `kill(pid: number, force?: boolean): boolean`

Sends a signal to the process. Returns `true` if the signal was sent successfully.

| `force` | Linux/macOS | Windows |
|---------|-------------|---------|
| `false` (default) | SIGTERM | `taskkill /PID` |
| `true` | SIGKILL | `TerminateProcess` |

### `killByPort(port: number, force?: boolean): number | null`

Convenience: finds the process on `port` and kills it. Returns the killed PID, or `null` if no process was found.

## Platform support

| Target | OS | Method |
|--------|----|--------|
| `x86_64-pc-windows-msvc` | Windows x64 | `GetExtendedTcpTable` / `GetExtendedUdpTable` |
| `aarch64-apple-darwin` | macOS ARM64 | `lsof -i :PORT` |
| `x86_64-unknown-linux-gnu` | Linux x64 (glibc) | `/proc/net` parsing |
| `x86_64-unknown-linux-musl` | Linux x64 (musl) | `/proc/net` parsing |
| `aarch64-unknown-linux-gnu` | Linux ARM64 | `/proc/net` parsing |

## License

MIT
