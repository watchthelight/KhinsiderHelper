# KhinsiderHelper

Desktop app for pulling video game soundtracks off [downloads.khinsider.com](https://downloads.khinsider.com). Tauri, Rust, TypeScript.

The original idea comes from [weespin/KhinsiderDownloader](https://github.com/weespin/KhinsiderDownloader); this is a ground-up rebuild as a native Windows app with account support baked in.

## What it does

Search the full khinsider catalog, browse album tracklists, pick a format (FLAC, MP3, OGG, or just let it grab the best available), and download. If you log in with your khinsider account, it pulls your personal downloads page so you can grab everything you own in one shot.

Parallel downloads are configurable from 1 to 8 simultaneous tracks. Already-downloaded files get skipped automatically, so you can pick up where you left off. Album art gets saved alongside the tracks.

## Building from source

Rust, Node.js 20+, pnpm.

```
cd app
pnpm install
pnpm tauri dev
pnpm tauri build
```

The installer ends up in `app/src-tauri/target/release/bundle/`.

## How it works under the hood

There's no official khinsider API, so the Rust backend scrapes the HTML directly. Search results, album metadata, track listings, download links; all parsed from the page DOM with CSS selectors. For authenticated features, a login window opens to khinsider's forum login page. Once you're through, the app extracts session cookies from the WebView and uses them going forward.

Downloads stream through Rust with progress events piped to the frontend in real time. Tracks land as `{album name}/{track number} {track name}.{ext}` under your output directory (defaults to `~/Music/Khinsider/`).

## Layout

```
app/
  src/              # TypeScript frontend (vanilla, no framework)
  src-tauri/src/    # Rust backend
    auth.rs         # WebView login, cookie extraction
    scraper.rs      # HTML parsing for search, albums, download links
    download.rs     # Streaming downloads with progress tracking
    models.rs       # Shared data structures
```

## License

MIT
