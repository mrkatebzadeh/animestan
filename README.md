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

## Quick usage

CLI:
```bash
cargo run --bin animestan-cli -- search "naruto"
```

TUI:
```bash
cargo run --bin animestan-tui
```
