# cadenza

`cadenza` is a terminal-first music player and visualizer for local audio libraries, built in Rust.

It focuses on:

- full-screen terminal playback
- recursive library browsing
- live spectrum rendering
- classical music-friendly presentation

## Install

Install the player from the repository root:

```bash
cargo install --path .
```

This installs the `cadenza` binary into your Cargo bin directory.

## Run

Scan the current directory:

```bash
cadenza
```

Scan a specific library directory:

```bash
cadenza ~/Music/cadenza
```

## Controls

- `j` / `k` or arrow keys: move selection
- `Enter`: expand directory or play file
- `h` / `l` or left/right: fold or unfold directories
- `Space`: pause or resume
- `q`: quit

## Supported formats

- `mp3`
- `flac`
- `wav`
- `ogg`

