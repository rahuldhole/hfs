# HFS Maintenance Guide

This document provides all the commands and directions needed to maintain and develop the HFS project.

---

## 📋 Prerequisites

Before working on this project, ensure you have:

- **Rust** (1.77+): [Install Rust](https://rustup.rs)
- **Node.js** (20+): [Install Node.js](https://nodejs.org)
- **Tauri CLI**: `cargo install tauri-cli`

---

## 🚀 Development Commands

### Start Development Server

```bash
# From project root
npx tauri dev
```

This starts both the Nuxt dev server (port 3000) and the Tauri app.

### Build for Production

```bash
# Build the production app
npx tauri build
```

Outputs are placed in `src-tauri/target/release/bundle/`.

### Rust Only (Backend)

```bash
cd src-tauri

# Check for errors
cargo check

# Run tests
cargo test

# Build release
cargo build --release
```

### Frontend Only

```bash
# Install dependencies
npm install

# Dev server
npm run dev

# Build
npm run build
```

---

## 📁 Project Structure

```
hfs/
├── app.vue                 # Main Vue component (server dashboard UI)
├── nuxt.config.ts          # Nuxt configuration
├── package.json            # Node dependencies
├── assets/                 # Screenshots and static assets
├── src-tauri/
│   ├── Cargo.toml          # Rust dependencies
│   ├── tauri.conf.json     # Tauri configuration
│   ├── capabilities/       # Tauri permissions
│   └── src/
│       ├── lib.rs          # Tauri commands & plugin setup
│       ├── http.rs         # HTTP server (Axum) + client UI
│       └── network.rs      # Network utilities
└── .github/
    └── workflows/
        └── release.yml     # Auto-release workflow
```

---

## 🔧 Key Configuration Files

| File | Purpose |
|------|---------|
| `src-tauri/tauri.conf.json` | App name, window size, bundle settings |
| `src-tauri/capabilities/default.json` | Tauri permissions (dialog, shell) |
| `nuxt.config.ts` | Nuxt/Vite configuration |
| `tailwind.config.ts` | Tailwind CSS settings |

---

## 🐛 Troubleshooting

### Port 3000 Already in Use

```bash
lsof -t -i:3000 | xargs kill -9
```

### Cargo Build Fails

```bash
cd src-tauri
cargo clean
cargo build
```

### Missing Tauri Plugins (Frontend)

```bash
npm install @tauri-apps/plugin-dialog @tauri-apps/plugin-shell
```

### Permission Errors

Check `src-tauri/capabilities/default.json` includes:
- `dialog:default`
- `shell:allow-open`

---

## 📦 Release Process

Releases are automated via GitHub Actions. To trigger a release:

```bash
git tag v1.0.0
git push origin v1.0.0
```

The workflow builds for macOS, Windows, and Linux, then uploads to GitHub Releases.

---

## 🔗 Useful Links

- [Tauri Documentation](https://tauri.app/v2/)
- [Nuxt Documentation](https://nuxt.com/docs)
- [Axum Documentation](https://docs.rs/axum)
- [Lucide Icons](https://lucide.dev/icons)
