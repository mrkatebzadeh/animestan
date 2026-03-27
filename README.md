<div align="center">
  <h1>ANIMESTAN</h1>
  <h4>CLI + TUI tool to search, watch, and keep track of animes</h4>
  <h5>Your terminal companion for anime discovery and tracking</h5>
  <p>
    <img src="https://img.shields.io/github/actions/workflow/status/mrkatebzadeh/animestan/ci.yml?branch=main&style=flat-square" alt="Build Status" />
    <img src="https://img.shields.io/codecov/c/github/mrkatebzadeh/animestan?style=flat-square" alt="Coverage" />
    <img src="https://img.shields.io/crates/v/animestan?style=flat-square" alt="Crates.io" />
  </p>
</div>

---

`animestan` is a terminal-based anime companion that comes in two flavors: a full-featured TUI for browsing, watching, and tracking your anime collection interactively, and a lightweight CLI designed for scripting and integration with tools like dmenu or rofi. Whether you want a rich terminal interface or a simple command-line tool, Animestan fits naturally into your workflow.

Built with extensibility and usability in mind, Animestan helps you search for anime, browse episodes, play or download them, and keep track of your watching progress, all from your terminal.

![Animestan TUI Screenshot](https://github.com/user-attachments/assets/2df9c9c7-632c-4f6f-bd57-ef005c53274c)

## Installation

### 1. Install script

Run the install script below to fetch the latest release for your platform.
   ```bash
   curl -fsSL https://mr.katebzadeh.xyz/tools/animestan/install | bash
   ```

### 2. Cargo install

If you have Rust and Cargo installed, you can install Animestan binaries directly:

```bash
cargo install animestan-cli
cargo install animestan
```

This installs the CLI (`animestan-cli`) and TUI (`animestan`) binaries globally.

### 3. Download from releases

Head to the [GitHub Releases page](https://github.com/mrkatebzadeh/animestan/releases) and download the appropriate archive for your platform:

- Linux x86_64 / aarch64
- macOS x86_64 / aarch64

Extract the archive and run the binary you want:

- `animestan-cli` — the CLI tool
- `animestan` — the full TUI app

### 4. Build from source

Clone the repo and build with Cargo:

```bash
git clone git@github.com:mrkatebzadeh/animestan.git
cd animestan
cargo build --release
```

The binaries will be located at:

```
target/release/animestan-cli
target/release/animestan
```

## Quick start

- Launch the full TUI interface with:

  ```bash
  animestan
  ```

- Use the CLI for scripting or dmenu/rofi integration:

  ```bash
  animestan-cli search "naruto"
  animestan-cli episodes <anime_id>
  animestan-cli play <episode_id>
  animestan-cli download <episode_id>
  animestan-cli bookmarks ls
  ```

- Example: pick an anime via dmenu and play an episode:

  ```bash
  anime_id=$(animestan-cli search "naruto" | dmenu | cut -f1)
  animestan-cli episodes "$anime_id" | dmenu | cut -f1 | xargs animestan-cli play
  ```

## Keybindings (TUI)

| Key        | Action                            |
| ---------- | --------------------------------- |
| `s`          | Enter search mode                 |
| `/`          | Enter filter mode                 |
| `q`          | Quit the application              |
| `j` / ↓      | Move down                         |
| `k` / ↑      | Move up                           |
| `Ctrl+k`     | Open quick launch                 |
| `Left`/`Right` | Toggle focus between panels       |
| `Tab`        | Cycle focus                       |
| `w`          | Mark current episode watched      |
| `u`          | Unmark current episode            |
| `m`          | Toggle bookmark for current anime |
| `W`          | Mark all episodes watched         |
| `U`          | Unmark all episodes               |
| `K`          | Mark episodes up to current       |
| `f`          | Cycle filter                      |
| `i`          | Show info modal                   |
| `d`          | Download current episode          |
| `D`          | Delete current episode            |
| `Space`      | Select current item               |
| `?`          | Show help                         |
| `Enter`      | Play episode or toggle focus      |
| `gg`         | Go to top                         |
| `G`          | Go to bottom                      |
| `M`          | Go to middle                      |
| `Ctrl+d`     | Half page down                    |
| `Ctrl+u`     | Half page up                      |

## Requirements

- `mpv` is required for playback.
- The CLI expects the configuration file to be located in the OS-specific config directory (e.g., `$HOME/.config/animestan/config.toml` on Linux).

## Configuration and customization

Animestan stores its configuration in a `config.toml` file located in the OS-specific config directory:

- Linux: `$HOME/.config/animestan/config.toml`
- macOS: `~/Library/Application Support/animestan/config.toml`

If the config file does not exist, Animestan creates one with default settings on first run.

### Main configuration keys

| Key                 | Description                                                                    |
| ------------------- | ------------------------------------------------------------------------------ |
| `source_id`           | Anime source to use (default: Allanime)                                        |
| `metadata_source`     | Metadata source for anime details (default: AllManga)                          |
| `metadata_cache_path` | Path to cached metadata (relative or absolute)                                 |
| `episodes_cache_path` | Path to cached episode lists (relative or absolute)                            |
| `player`              | Media player command (default: `mpv`)                                            |
| `quality`             | Streaming quality preference: `best`, `worst`, or specific quality (default: `best`) |
| `tracking_path`       | Path to episode tracking file (relative or absolute)                           |
| `favorites_path`      | Path to favorites file (relative or absolute)                                  |

Feel free to customize these settings by editing the `config.toml` file to tailor Animestan to your preferences.

---

Enjoy your anime journey with Animestan!
