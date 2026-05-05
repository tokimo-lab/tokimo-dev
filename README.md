# @tokimo-lab/port-ops

[![CI](https://github.com/tokimo-lab/tokimo-dev/actions/workflows/CI.yml/badge.svg)](https://github.com/tokimo-lab/tokimo-dev/actions/workflows/CI.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

Cross-platform native addon for port and process operations. Zero JS dependencies — pure Rust via [napi-rs](https://napi.rs).

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
import {
  findPidByPort, findPidByPortAll, isPortAvailable,
  waitForPortFree, findPidsByName,
  kill, killTree, killByPort,
} from '@tokimo-lab/port-ops'

// Find which process owns port 3000
const pid = findPidByPort(3000)       // number | null

// Get ALL PIDs on a port (TCP + UDP, IPv4 + IPv6)
const pids = findPidByPortAll(3000)   // number[]

// Check if a port is free
isPortAvailable(3000)                 // boolean

// Wait for a port to become free (polls every 50ms)
waitForPortFree(3000, 5000)           // boolean (true = free, false = timeout)

// Find processes by name (case-insensitive substring)
findPidsByName('node')                // number[]

// Kill a process
kill(pid)                             // SIGTERM / taskkill
kill(pid, true)                       // SIGKILL / TerminateProcess

// Kill a process AND all its children (process tree)
killTree(pid)                         // graceful
killTree(pid, true)                   // force

// Find + kill in one call
killByPort(3000)                      // number | null
killByPort(3000, true)                // force
```

### `findPidByPort(port: number): number | null`

Returns the PID of the process listening on `port`, or `null` if no process is found. Searches TCP (LISTEN state) and UDP on both IPv4 and IPv6.

### `findPidByPortAll(port: number): number[]`

Returns all PIDs listening on `port`. Unlike `findPidByPort`, this returns every match, not just the first.

### `isPortAvailable(port: number): boolean`

Returns `true` if no process is listening on `port`.

### `waitForPortFree(port: number, timeoutMs: number): boolean`

Polls until the port has no LISTEN socket. Returns `true` if the port became free, `false` on timeout. Polls every 50ms.

### `findPidsByName(name: string): number[]`

Finds all PIDs whose process name contains `name` (case-insensitive). Returns an empty array if no match is found.

| Platform | Method |
|----------|--------|
| Windows | `CreateToolhelp32Snapshot` + `Process32First/Next` |
| Linux | `/proc/{pid}/comm` |
| macOS | `pgrep -if` |

### `kill(pid: number, force?: boolean): boolean`

Sends a signal to the process. Returns `true` if the signal was sent successfully.

| `force` | Linux/macOS | Windows |
|---------|-------------|---------|
| `false` (default) | SIGTERM | `taskkill /PID` |
| `true` | SIGKILL | `TerminateProcess` |

### `killTree(pid: number, force?: boolean): boolean`

Kills a process and all its children recursively (process tree). Returns `true` if the root process was killed.

| Platform | Method |
|----------|--------|
| Windows | Enumerate processes via ToolHelp, build parent→children tree, kill bottom-up |
| Linux | Parse `/proc/{pid}/stat` for PPid, build tree, kill leaves-first |
| macOS | `pgrep -P` to find children, recurse, kill leaves-first |

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
