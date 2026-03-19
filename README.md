<div align="center">

# 🌐 HFS – HTTP File Share
### *Share files instantly over your local network.* 
***No cloud. No accounts. No friction. Just drop and go.***

[![Release](https://img.shields.io/github/v/release/rahuldhole/hfs?style=for-the-badge&logo=github&color=3b82f6)](https://github.com/rahuldhole/hfs/releases/latest)
[![Downloads](https://img.shields.io/github/downloads/rahuldhole/hfs/total?style=for-the-badge&logo=github&color=10b981)](https://github.com/rahuldhole/hfs/releases)
[![License](https://img.shields.io/github/license/rahuldhole/hfs?style=for-the-badge&color=f59e0b)](LICENSE.md)

[📥 Download for macOS](#-download) • [📥 Download for Windows](#-download) • [📥 Download for Linux](#-download)

---

### *"The missing piece in my local workflow. Moving files between my workstation and test devices is finally instant."*

</div>

## 📥 Download

HFS is available for all major platforms. Download the latest installer below:

| OS | Package |
|:---|:---|
| **macOS (Silicon)** | [**HFS_aarch64.dmg**](https://github.com/rahuldhole/hfs/releases/latest/download/HFS_aarch64.dmg) |
| **macOS (Intel)** | [**HFS_x64.dmg**](https://github.com/rahuldhole/hfs/releases/latest/download/HFS_x64.dmg) |
| **Windows** | [**HFS_x64.msi**](https://github.com/rahuldhole/hfs/releases/latest/download/HFS_x64.msi) |
| **Linux (Debian)** | [**HFS_amd64.deb**](https://github.com/rahuldhole/hfs/releases/latest/download/HFS_amd64.deb) |
| **Linux (AppImage)** | [**HFS_amd64.AppImage**](https://github.com/rahuldhole/hfs/releases/latest/download/HFS_amd64.AppImage) |

> [!IMPORTANT]
> **🍎 macOS Note**: Since HFS is open-source and not code-signed, macOS may show a security alert. To open:  
> Right-click the app → **Open** → **Open** in dialog.  
> *Or run:* `sudo xattr -cr /Applications/HFS.app`

---

## ✨ Why HFS?

HFS turns your local machine into a temporary file server. Anyone on your local network can access and download your shared files via their web browser—no client-side app required.

### 🚀 Key Features
- **One-Click Sharing**: Drop files or folders and start the server instantly.
- **Universal Access**: Recipients use any browser on Phones, Tablets, TVs, or PCs.
- **Batch Downloads**: Download entire directories or multiple files as a single ZIP.
- **Blazing Performance**: Built with **Rust/Axum** and **Tauri** for minimal overhead.
- **Private & Secure**: Your files never leave your network. Zero tracking. Zero cloud.

---

## 📸 See it in action

<div align="center">

### Control Center (Server)
*Manage your shared content with ease*

![Server Dashboard](public/assets/server-dashboard.png)

### Recipient View (Client)
*A beautiful, responsive web interface for downloading*

![Client Interface](public/assets/client-interface.png)

</div>

---

## 🚀 Get Started in Seconds

1. **Download** the app for your platform [above](#-download).
2. **Launch** and choose your files or folders to share.
3. **Start Server** to generate a unique LAN address.
4. **Share the Link** with anyone on your local network.
5. **Done!** They browse and download immediately.

---

<div align="center">
  <h2>🛠️ Developer Resources</h2>
</div>

### ⚙️ Tech Stack
- **Engine**: [Rust 1.77+](https://rust-lang.org) + [Axum](https://github.com/tokio-rs/axum)
- **Frontend**: [Nuxt 4](https://nuxt.com) + [Vue 3](https://vuejs.org) + [Tailwind CSS](https://tailwindcss.com)
- **Desktop**: [Tauri 2](https://tauri.app)
- **Icons**: [Lucide](https://lucide.dev)

### 🛠️ Development Setup
1. `git clone https://github.com/rahuldhole/hfs.git`
2. `pnpm install`
3. `pnpm tauri dev`

*See [MAINTENANCE.md](MAINTENANCE.md) for deeper technical insights.*

### 🤝 Contributing
Contributions are always welcome! Feel free to open a PR or report issues on GitHub.

### 📄 License
This project is licensed under the MIT License - see [LICENSE.md](LICENSE.md) for details.

---

<div align="center">

**Crafted with ❤️ by [Rahul Dhole](https://rahuldhole.com)**  
*Making local file sharing simple again.*

</div>
