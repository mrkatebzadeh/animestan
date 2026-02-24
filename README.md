# Animestan

CLI + TUI weapon to search, watch, and keep track of animes.

## Why Animestan?

I’ve used ani-cli for a long time and loved how useful it is, but I kept missing one thing: a way to track what I’ve watched. I ended up building my own version so I could add tracking and keep it extensible. Animestan comes in two flavors: a CLI you can wire into a dmenu/rofi workflow, and a full TUI when you want the complete interface.

## Getting started

### Download a stable release

1. Go to the GitHub **Releases** page.
2. Download the archive for your platform:
   - Linux x86_64 / aarch64
   - macOS x86_64 / aarch64
3. Extract the archive and run the binary you want:
   - `animestan-cli`
   - `animestan-tui`

### Build from source

```bash
git clone git@github.com:mrkatebzadeh/animestan.git
cd animestan
cargo build --release
```

The binaries will be in:

```
target/release/animestan-cli
target/release/animestan-tui
```

## Requirements

- `mpv` is currently required for playback.

## Configuration

Animestan stores configuration in the OS-specific config directory, under an `animestan/config.toml` file. On Linux this resolves to `$HOME/.config/animestan/config.toml`.

## TUI

The TUI provides the full interactive experience (search, browse, playback, and tracking) in a single terminal view.

![Image](https://github.com/user-attachments/assets/e85d30e9-564c-40fa-bad5-196379dede2b)

## CLI

`animestan-cli` is a structured, script-friendly interface intended for piping into tools like **dmenu** or **rofi**, not a fully interactive experience like `ani-cli`. The CLI exposes **search**, **episodes**, **url**, **play**, **download**, **delete**, and **bookmarks** commands.

The workflow is simple:

- `search` returns **anime IDs**
- `episodes` returns **episode IDs**
- the remaining commands operate on those IDs

### Examples

**Search for an anime (returns anime IDs):**
```bash
animestan-cli search "naruto"
# output: <anime_id>\t<title>
```

**List episodes for an anime (returns episode IDs):**
```bash
animestan-cli episodes <anime_id>
# output: <episode_id>\t<episode_number>\t<title>
```

**Get the stream URL for an episode:**
```bash
animestan-cli url <episode_id>
```

**Play an episode:**
```bash
animestan-cli play <episode_id>
```

**Download an episode:**
```bash
animestan-cli download <episode_id>
```

**Delete a downloaded episode:**
```bash
animestan-cli delete <episode_id>
```

**Bookmarks workflow:**
```bash
animestan-cli bookmarks ls
animestan-cli bookmarks add <anime_id>
animestan-cli bookmarks rm <anime_id>
```

### dmenu/rofi integration idea

**Pick an anime ID via dmenu, then list episodes:**
```bash
anime_id=$(animestan-cli search "naruto" | dmenu | cut -f1)
animestan-cli episodes "$anime_id" | dmenu | cut -f1 | xargs animestan-cli play
```

## Quick usage

CLI:
```bash
cargo run --bin animestan-cli -- search "naruto"
```

TUI:
```bash
cargo run --bin animestan-tui
```
