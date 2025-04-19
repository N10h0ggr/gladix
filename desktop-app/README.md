
# Gladix Desktop App

This is the desktop client for the Gladix project, built with **Tauri**, **React**, **TypeScript**, **Vite**, and **Tailwind CSS**. It provides a cross-platform, lightweight GUI interface.

---

## 🚀 Getting Started

### Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)


### Prerequisites

Make sure the following tools are installed on your system:

- [Node.js](https://nodejs.org/) (LTS recommended)
- [Rust](https://www.rust-lang.org/tools/install)
- [Tauri CLI](https://tauri.app/v1/guides/getting-started/prerequisites/)

Install Tauri CLI globally:

```bash
cargo install tauri-cli
```

---

### 📦 Installation

Clone the repository and navigate to the desktop app folder:

```bash
git clone https://github.com/N10h0ggr/gladix.git
cd gladix/desktop-app
```

Install Node.js dependencies:

```bash
npm install
```

---

### 🧪 Running in Development

Launch the desktop app in development mode:

```bash
npm run tauri dev
```

This will start the frontend development server and launch the Tauri window.

---

### 🛠 Build for Production

To generate a production-ready native binary:

```bash
npm run tauri build
```

---

## 🧰 Tech Stack

- [Tauri](https://tauri.app/)
- [React](https://reactjs.org/)
- [TypeScript](https://www.typescriptlang.org/)
- [Vite](https://vitejs.dev/)
- [Tailwind CSS](https://tailwindcss.com/)

---

## 📁 Project Structure

```
desktop-app/
├── public/           # Static assets
├── src/              # React source files
├── src-tauri/        # Tauri backend configuration and Rust code
├── vite.config.ts    # Vite config
├── tailwind.config.ts# Tailwind CSS config
└── package.json      # Project metadata and scripts
```
