# বাংলা স্ক্রিনসেভার 🕐

A beautiful Bangla digital clock screensaver for Windows featuring the traditional Bengali calendar (বঙ্গাব্দ), Bangla numerals, and seasonal greetings.

<div align="center">

![Bangla Screensaver](https://img.shields.io/badge/Bangla-Screensaver-blue?style=for-the-badge)
![Windows](https://img.shields.io/badge/Windows-0078D6?style=for-the-badge&logo=windows&logoColor=white)
![Rust](https://img.shields.io/badge/Rust-000000?style=for-the-badge&logo=rust&logoColor=white)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg?style=for-the-badge)](https://opensource.org/licenses/MIT)

</div>

---

## ✨ Features

- 🕐 **Bangla Digital Clock** – Displays time with elegant Bangla numerals (০১২৩৪৫৬৭৮৯)
- 📅 **Bengali Calendar** – Shows date in বঙ্গাব্দ with accurate day/month calculations
- 🌸 **Seasonal Display** – Shows the current Bengali season (ষড়ঋতু)
- 🇧🇩 **Regional Support** – Choose between Bangladesh and India calendar conventions
- ⏰ **Timezone Awareness** – Calculates Bengali date based on Bangladesh/India time
- ⚙️ **Configurable** – Customize colors, fonts, and display options
- 💨 **Lightweight** – Pure Rust with minimal dependencies (~2MB)

---

## 📸 Preview

![Bangla Screensaver Preview](screenshot.png)

---

## 📥 Installation

### Option 1: Download Release (Recommended)

1. Go to [**Releases**](https://github.com/abusayed0206/bsaver/releases)
2. Download the latest `bsaver.scr` file
3. Right-click `bsaver.scr` → **Install**
4. Or copy to `C:\Windows\System32\` for system-wide installation

### Option 2: Build from Source

**Prerequisites:**
- [Rust](https://rustup.rs/) (stable, 2024 edition)
- Windows 10/11

```powershell
# Clone the repository
git clone https://github.com/abusayed0206/bsaver.git
cd bsaver

# Build release version
cargo build --release

# The screensaver will be at: target\release\bsaver.exe
# Rename to .scr for Windows to recognize it as a screensaver
copy target\release\bsaver.exe bsaver.scr
```

---

## 🚀 Usage

### Run as Screensaver

1. **Install**: Right-click `bsaver.scr` → **Install**
2. Go to **Settings** → **Personalization** → **Lock screen** → **Screen saver settings**
3. Select **"Bangla Screensaver"** from the dropdown
4. Click **Preview** to test, **Settings** to configure

### Configure Settings

**Via Settings Dialog:**
- Right-click `bsaver.scr` → **Configure**
- Or from Screen Saver Settings → **Settings** button

**Via Command Line:**
```powershell
# Preview screensaver
cmd /c "bsaver.scr /p"

# Open settings dialog
cmd /c "bsaver.scr /c"

# Run screensaver full-screen
cmd /c "bsaver.scr /s"
```

### Configuration File

Settings are stored at:
```
%APPDATA%\bsaver\bsaver\config.json
```

**Example Configuration:**
```json
{
  "time_color": [255, 255, 255],
  "date_color": [200, 200, 200],
  "day_season_color": [180, 180, 180],
  "background_color": [0, 0, 0],
  "time_font_size": 120,
  "date_font_size": 60,
  "day_season_font_size": 45,
  "show_seconds": true,
  "show_date": true,
  "show_day_season": true,
  "calendar_region": "Bangladesh"
}
```

---

## 🌍 Calendar Region

Choose between **Bangladesh** and **India** calendar conventions:

| Setting | Pohela Boishakh | Timezone |
|---------|-----------------|----------|
| Bangladesh | April 14 | UTC+6 |
| India | April 15 | UTC+5:30 |

The Bengali date is always calculated based on the selected region's timezone, not your local time.

---

## 🎨 Customization

| Option | Description | Default |
|--------|-------------|---------|
| `time_color` | RGB color for clock | White `[255,255,255]` |
| `date_color` | RGB color for date | Light gray `[200,200,200]` |
| `background_color` | RGB background | Black `[0,0,0]` |
| `time_font_size` | Clock font size | `120` |
| `show_seconds` | Display seconds | `true` |
| `show_date` | Display Bengali date | `true` |
| `show_day_season` | Display day & season | `true` |
| `calendar_region` | `"Bangladesh"` or `"India"` | `"Bangladesh"` |

---

## 🛠️ Development

### Build Commands

```powershell
# Development build
cargo build

# Release build (optimized)
cargo build --release

# Run tests
cargo test

# Check for issues
cargo clippy
```

### Project Structure

```
bsaver/
├── src/
│   ├── main.rs          # Entry point, command parsing
│   ├── screensaver.rs   # Windows screensaver API
│   ├── renderer.rs      # Text rendering with cosmic-text
│   ├── clock.rs         # Clock layout and formatting
│   ├── bangla_date.rs   # Bengali calendar calculations
│   ├── config.rs        # Configuration management
│   └── settings.rs      # Settings dialog UI
├── Cargo.toml
└── README.md
```

---

## 📋 System Requirements

- **OS**: Windows 10/11
- **Display**: Any resolution (auto-scales)
- **Memory**: ~12MB private / ~24MB working set (scales with resolution)
- **Disk**: ~2MB

> **Note**: Memory optimizations include embedded-only font loading (no system fonts loaded), reusable frame buffer, and periodic glyph cache cleanup. The frame buffer uses ~8MB at 1080p.

---

## 🤝 Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/AmazingFeature`)
3. Commit your changes (`git commit -m 'Add some AmazingFeature'`)
4. Push to the branch (`git push origin feature/AmazingFeature`)
5. Open a Pull Request

---

## 📜 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

---

## 🙏 Credits

- **Font**: [Ekush](https://codepotro.com/font/ekush/)


---

<div align="center">

**Made with ❤️ for the Bengali community**

[⭐ Star this project](https://github.com/abusayed0206/bsaver) if you find it useful!

</div>
