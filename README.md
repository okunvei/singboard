# Singboard

Singboard is a `Tauri + Vue 3` desktop dashboard for sing-box. It provides a GUI for Clash API data, proxy groups, connections, rules, and service management.

## Tech Stack

- Frontend: `Vue 3`, `TypeScript`, `Vite`, `TailwindCSS`, `DaisyUI`, `ECharts`
- Backend: `Rust`, `Tauri 2`
- Platform: currently focused on `Windows` (with Windows service integration)

## Features

- Overview: traffic, memory, IP info, latency, connection topology
- Proxies: proxy group switching, node latency test, provider update and health check
- Rules: rules and rule provider listing/updating
- Connections: live connection table and disconnect actions
- Logs: live logs with filtering
- Settings:
  - sing-box service install, start, stop, restart, uninstall
  - Clash mode switching based on core-reported mode list
  - Clash API URL and secret configuration
  - Latency URL and IPv6 test toggle
  - Core path, config path, and working directory setup
  - Theme switching (`light`, `dark`, `dracula`, `nord`)
- First-run setup wizard for path and Clash API initialization

## Requirements

- Node.js 18+
- `pnpm`
- Rust stable (recommended via `rustup`)
- Windows C++ build tools (MSVC toolchain)
- WebView2 Runtime
- sing-box binary and config file (configured in Settings or detected by setup wizard)

## Development

Install dependencies:

```bash
pnpm install
```

Run frontend dev server:

```bash
pnpm dev
```

Run Tauri app in development mode:

```bash
pnpm tauri dev
```

## Build

Build frontend:

```bash
pnpm build
```

Build desktop app:

```bash
pnpm tauri build
```

## Project Structure

```text
.
├─ src/                 # Vue frontend
├─ src-tauri/           # Rust + Tauri backend
├─ public/
├─ package.json
└─ src-tauri/tauri.conf.json
```

## Key Configuration

- Tauri identifier in `src-tauri/tauri.conf.json` (`identifier`)
- Clash API and runtime paths in the in-app Settings page
- Default landing route: `/proxies`

## Local Data Storage

App state is persisted via WebView local storage and cache:

- Typical path: `%LOCALAPPDATA%\\singboard\\EBWebView\\Default\\`
- `Local Storage\\leveldb` stores frontend `localStorage` data

If you used an old app identifier before, old and new storage folders may exist at the same time.

## Notes

- This dashboard relies on Clash API. Make sure sing-box exposes the API and is reachable.
- Default Windows service name is `sing-box`, and can be changed in Settings.


## License

[GPL-3.0](LICENSE)

---

> 🤖 **AI-Assisted Development**
>
> This project is an exploration of AI's project-level coding capabilities. Code generated using **Claude Code(Claude Opus 4.6)** and **GPT-5.3-Codex** models via **Visual Studio Code**.
>