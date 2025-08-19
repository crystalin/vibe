# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Vibe is an offline audio/video transcription application built with Tauri (Rust backend + React frontend). It uses OpenAI Whisper models for transcription with support for multiple hardware accelerations (GPU, Vulkan, CoreML, CUDA, ROCm).

## Architecture

### Multi-workspace Structure
- **Root Workspace**: Cargo workspace managing all Rust components
- **Core Library** (`core/`): Rust library with transcription logic, audio processing, and Whisper integration
- **Desktop App** (`desktop/`): Tauri application with React frontend and Rust backend
- **Landing Page** (`landing/`): SvelteKit marketing website

### Key Components
- **Rust Backend**: Tauri app handling audio processing, file operations, model management, and system integration
- **React Frontend**: UI built with TypeScript, Tailwind CSS, and DaisyUI components
- **Core Engine**: Whisper-based transcription with hardware acceleration support
- **Internationalization**: Multi-language support with JSON locale files

## Development Commands

### Desktop Application
```bash
cd desktop
bun install                    # Install frontend dependencies
bun run dev                    # Start development server
bun run build                  # Build frontend for production
bun run lint                   # Run ESLint
bunx tauri dev                 # Start Tauri development mode
bunx tauri build               # Build desktop application
```

### Rust Components
```bash
cargo fmt                      # Format Rust code
cargo clippy                   # Run linter
cargo test --release          # Run tests in release mode
cargo test -p vibe_core --release -- --nocapture  # Test core library
```

### Build Setup
```bash
bun run scripts/pre_build.js  # Execute pre-build scripts (required before building)
```

### Landing Page
```bash
cd landing
bun run dev                    # Start SvelteKit development
bun run build                  # Build static site
bun run lint                   # Run linter
bun run format                 # Format code
```

## Hardware Acceleration Features

The application supports multiple GPU acceleration backends:
- **CUDA**: Nvidia GPU support (Windows/Linux)
- **Vulkan**: Cross-platform GPU acceleration
- **CoreML**: Apple Silicon optimization (macOS)
- **ROCm**: AMD GPU support (Linux only)
- **Metal**: Apple GPU acceleration (macOS)

Features are controlled via Cargo feature flags in `desktop/src-tauri/Cargo.toml`.

## Testing Strategy

- Core library tests use `serial_test` for sequential execution
- Run tests with `RUST_LOG=trace` for detailed logging
- Use `--release` mode for performance-critical tests
- Frontend has no dedicated test suite currently

## Configuration Files

- `tauri.conf.json`: Main Tauri configuration
- Platform-specific configs: `tauri.{windows,macos,linux}.conf.json`
- Localization files in `desktop/src-tauri/locales/`
- Rust workspace configuration in root `Cargo.toml`

## Development Notes

- Frontend uses Bun as package manager, not npm/yarn
- Rust code requires pre-built FFmpeg and OpenBLAS dependencies
- Multi-language support via i18next with JSON locale files
- The app supports CLI mode with `--help` flag
- Server mode available with `--server` flag and Swagger docs at `/docs`

## File Locations

- Main Rust entry: `desktop/src-tauri/src/main.rs`
- Core transcription logic: `core/src/`
- React app entry: `desktop/src/App.tsx`
- Tauri commands: `desktop/src-tauri/src/cmd/`
- Frontend components: `desktop/src/components/`
- Build documentation: `docs/building.md`