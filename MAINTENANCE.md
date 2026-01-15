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
pnpm install

# Dev server
pnpm dev
npx tauri dev

# Build
pnpm build
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
pnpm install @tauri-apps/plugin-dialog @tauri-apps/plugin-shell
```

### Permission Errors

Check `src-tauri/capabilities/default.json` includes:
- `dialog:default`
- `shell:allow-open`

---

## 📦 Release Process

We use `package.json` as the single source of truth for the application version.

1. **Bump Version**: Update the version in `package.json`.
2. **Sync Version**: Run the sync script to update Tauri configuration files.

```bash
# Example: Bumping to 0.0.1
npm version 0.0.1 --no-git-tag-version
pnpm run sync-version
```

3. **Commit & Tag**:

```bash
git add .
git commit -m "chore: release v0.0.1"
git tag v0.0.1
git push origin v0.0.1
```

4. **Automated Release**: The GitHub Actions workflow will detect the tag and build releases for all platforms.

---

## 🔗 Useful Links

- [Tauri Documentation](https://tauri.app/v2/)
- [Nuxt Documentation](https://nuxt.com/docs)
- [Axum Documentation](https://docs.rs/axum)
- [Lucide Icons](https://lucide.dev/icons)
