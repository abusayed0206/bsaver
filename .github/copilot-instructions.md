# Copilot Instructions – bsaver

## Project Overview

**bsaver** is a Windows screensaver (`.scr`) displaying a Bangla digital clock with Bengali calendar (বঙ্গাব্দ) support. Written in Rust (2024 edition), it uses raw Win32 APIs via `windows-rs` for window management and `cosmic-text` for Bangla text shaping/rendering. There is no GPU rendering — all drawing is CPU-based to a BGRA pixel buffer blitted via GDI.

## Architecture

Six modules with clear responsibilities — all wired through `main.rs`:

- **`main.rs`** — Entry point. Parses `/s`, `/p <hwnd>`, `/c` args into `ScreensaverMode` and dispatches.
- **`screensaver.rs`** — Win32 window creation, message loop, double-buffered GDI rendering. Owns the global `Renderer` via `OnceLock<Mutex<Renderer>>`. Uses a thread-local `RENDER_BUFFER` to avoid per-frame heap allocations.
- **`renderer.rs`** — Text rendering with `cosmic-text`. Loads only the embedded Ekush font (no system fonts). Provides `render_text()` (BGRA output), `render_text_centered()`, and `render_time_fixed_grid()` (fixed-width digit cells to prevent clock jitter). Caches digit widths and periodically resets `SwashCache` to bound memory.
- **`clock.rs`** — Formats time, date, day, season strings. Handles 12h/24h, Bangla/English numerals and names. Region-aware via `Config`.
- **`bangla_date.rs`** — Gregorian-to-Bengali calendar conversion. Handles Bangladesh (Apr 14 Pohela Boishakh, UTC+6) vs India (Apr 15, UTC+5:30) conventions. Always converts to the region's timezone first, not local system time. Has comprehensive tests.
- **`config.rs`** — `Config` struct with serde JSON serialization. Stored at `ProjectDirs::from("dev", "abusayed", "bsaver")`. Uses `let-chain` syntax for loading.
- **`settings.rs`** — Native Win32 settings dialog built with `CreateWindowExW` toggle buttons.
- **`launcher.rs`** — Separate binary (`BanglaSaver`) providing a launcher UI to register/unregister the screensaver via HKCU registry. Uses `thread_local!` + `Cell` for Win32 UI state (Rust 2024 forbids `static mut`). Registry writes use `reg.exe` (not `RegSetValueExW`) to bypass MSIX virtualization.

## Two Binaries

Defined in `Cargo.toml`:
- `bsaver` (`src/main.rs`) — The screensaver itself
- `BanglaSaver` (`src/launcher.rs`) — Launcher/installer UI

Both use `#![windows_subsystem = "windows"]` to hide the console.

## Key Patterns

- **No `static mut`**: Rust 2024 edition. Use `OnceLock`, `LazyLock`, `thread_local!` with `Cell`/`RefCell`, or `Mutex` for shared state.
- **Embedded font**: The Ekush font is included via `include_bytes!("../font/Ekush-Regular.ttf")` — no runtime font loading or system font enumeration.
- **BGRA pixel buffers**: All rendering goes to `Vec<u8>` in BGRA format, then `SetDIBitsToDevice` to GDI. The screen buffer is thread-local and never shrinks.
- **Fixed-width time grid**: `render_time_fixed_grid()` measures the widest digit and centers each character in a fixed cell to prevent layout shifts when digits change.
- **Timezone-first date calculation**: `BanglaDate::from_local_with_region()` converts local time → UTC → region timezone before calculating the Bengali date. This is intentional — see tests in `bangla_date.rs`.
- **Memory discipline**: SwashCache cleanup every 500 renders, no system font loading, reusable buffers. Target: ~12MB private working set at 1080p.
- **MSIX registry bypass**: The launcher uses `std::process::Command` to invoke `reg.exe` for all `HKCU\Control Panel\Desktop` writes/reads/deletes. Direct `RegSetValueExW`/`RegQueryValueExW` calls are virtualized inside an MSIX container, so the Windows screensaver service would never see them. `reg.exe` is a system binary outside the MSIX package, so its writes go to the real registry.
- **Static CRT linking**: `.cargo/config.toml` sets `+crt-static` to eliminate the MSVC CRT (`vcruntime140.dll`) runtime dependency for Store distribution.

## Build & Test

```powershell
cargo build                    # Dev build
cargo build --release          # Optimized release (~2MB)
cargo test --verbose           # Run tests (bangla_date has timezone/calendar tests)
cargo clippy -- -D warnings    # Lint (CI enforces zero warnings)
cargo fmt --all -- --check     # Format check
```

Release profile uses `lto = true`, `codegen-units = 1`, `panic = "abort"`, `strip = true`.

To package as MSIX: `.\packaging\build-msix.ps1` (requires Windows 10 SDK for `MakeAppx.exe`).

## CI

GitHub Actions (`windows-latest` only): test → clippy → fmt → build + MSIX. The build job uploads `bsaver.exe`, `BanglaSaver.exe`, and `.msix` artifacts. Releases trigger on `v*` tags.

## When Modifying

- **Adding display elements**: Add config field in `config.rs` → format in `clock.rs` → render in `screensaver.rs::render_clock_content()` → toggle in `settings.rs`.
- **Calendar logic**: All date math is in `bangla_date.rs`. Month lengths follow the 2019 revised Bangladesh calendar (first 5 months = 31 days). Add tests covering multiple timezones.
- **Win32 APIs**: All `unsafe` blocks must be explicit (Rust 2024). Use `windows-rs` typed wrappers. Clean up GDI resources (`DeleteObject`, `DeleteDC`) after use.
- **Font changes**: Replace `font/Ekush-Regular.ttf` and the `include_bytes!` path. Bangla shaping requires `Shaping::Advanced` in cosmic-text.
