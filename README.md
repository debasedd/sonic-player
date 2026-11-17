# Sonic

A minimal, floating **Liquid Glass** media controller for Windows.

Sonic does not play audio. It is a controller: the sound keeps coming from whatever
app or device is already playing, and Sonic gives you a small always-on-top pill to
pause, skip, seek, and change volume without leaving what you are doing.

```
┌──────────────────────────────────────────┐
│  ▣   Midnight City            ◀  ▶  ▶   │   ← pill
│      M83 · Hurry Up, We're Dreaming      │
│      ──────●──────────────  1:12  -3:04  │
└──────────────────────────────────────────┘
```

Two control paths, in priority order:

1. **Local Windows media** (GSMTC — the same session the media keys and the
   flyout use). Works with Spotify, Apple Music, the Windows Media Player, a
   browser tab, anything that registers a media session. No account needed.
2. **Spotify Web API** — optional, for device picking and finer control when you
   want it.

---

## Features

**Player**

- Play/pause, previous/next, seek, shuffle, repeat
- Album artwork with crossfade, sampled ambient colour, and a graceful fallback
- Three sizes: mini (248×60), pill (416×108), expanded (416×548)
- Hover the island to grow it from mini; leave and it settles back
- Always-on-top, frameless, transparent, centred at the top of the monitor
- Position memory between sessions
- Tray icon with a context menu

**Pocket** — a utility page beside the player

- **File Shelf** — drag files from Explorer onto the island; Sonic keeps a copy and
  never touches the original. Drag a copy back out to Explorer any time.
- **Clipboard Vault** — optional local history of what you copy, with private-mode
  filtering for passwords, tokens and card numbers.
- Pocket is a *page*, not a bigger window: scroll down or drag left to slide into
  it, scroll up or drag right to slide back. The window keeps its size.

**Material**

- Liquid Glass: blurred album wash, specular sheen, cursor light, inset rim light
- Optional **Solid** mode — a flat black pill like the iPhone Dynamic Island
- Fully responsive down to the mini strip

**Privacy**

- Refresh tokens live in Windows Credential Manager (DPAPI, per user)
- The webview never sees a token — it receives a state snapshot and sends commands
- Spotify uses Authorization Code + PKCE; no client secret is used or needed

---

## Requirements

- Windows 10 (1809+) or Windows 11
- [Rust 1.77+](https://rustup.rs/) (MSVC toolchain)
- [Node.js 18+](https://nodejs.org/)
- Visual Studio Build Tools 2022 with the Windows 11 SDK — required for the MSVC
  linker on a fresh machine
- [WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/) —
  present on Windows 11 and on updated Windows 10

---

## Quick start

```powershell
npm install
npm run tauri dev        # development
```

Build a release and an NSIS installer:

```powershell
npm run tauri build
```

The installer lands at
`src-tauri/target/release/bundle/nsis/Sonic_<version>_x64-setup.exe`.

### Spotify (optional)

Sonic works without any Spotify setup via local Windows media. To enable Spotify
Connect device picking and the Web API path:

1. Create an app at <https://developer.spotify.com/dashboard>
2. Add `http://127.0.0.1:8976/callback` to its redirect URIs
3. Copy `.env.example` to `.env` and set `SONIC_SPOTIFY_CLIENT_ID`

The Client ID can also be pasted straight into Settings, which is the easier route
for a release build. See [SETUP.md](./SETUP.md).

---

## Keyboard

| Key | Action |
| --- | --- |
| `Space` | Play / pause |
| `Shift` + `←` / `→` | Previous / next |
| `←` / `→` (scrubber focused) | Seek ∓5 s (`Shift` = 30 s) |
| `M` | Toggle mini |
| `E` | Toggle expanded |
| `H` | Hide to tray |
| `Ctrl` + `,` | Settings |
| `Esc` | Close the open panel |

Shortcuts are suppressed while a form field has focus, and `Space`/`Enter` are left
to a focused button so a keyboard user who tabbed to "Next track" does not trigger
both the button and the transport.

---

## Architecture

Sonic is split so that no Spotify secret ever reaches the webview.

```
┌──────────────────────────── Tauri IPC ────────────────────────────┐
│                                                                    │
│   React + TS (webview)                    Rust (src-tauri)         │
│   ─────────────────────                   ──────────────────        │
│   components/            invoke() ──────► commands.rs              │
│   hooks/                 ◄── listen() ─── player.rs   (poller)     │
│   store/ (zustand)                        local_media.rs (GSMTC)   │
│   services/spotify/api.ts                 spotify.rs  (ureq)       │
│                                           auth.rs     (PKCE)       │
│                                           vault.rs    (Credential) │
│                                           shelf.rs    (files)      │
│                                           clipboard.rs             │
│                                           drag_drop.rs (OLE)       │
│                                           window.rs   (geometry)   │
│                                           hover.rs    (OS cursor)  │
└────────────────────────────────────────────────────────────────────┘
```

**Rust owns the geometry.** The window rect is never written by the webview —
`window::animate_to_mode` tweens size and position together on the main thread with
a damped spring, and the UI only draws. That removed the whole class of "DOM and
window disagree" bugs, including the panel-height flicker where an open Pocket
collapsed to mini while the cursor was still on it.

**Pocket is a page at the pill's own rect**, not a taller window. Player and Pocket
are two pages in one translating track, so the window never resizes for a panel and
the gesture is reversible mid-drag.

**Native drag and drop, both ways.** Dropping from Explorer is handled by Tauri's
WebView2 handler (`drag_drop.rs`); dragging a shelf file back out is a real OLE drag
(`pocket.rs` + the `drag` crate), so Explorer receives actual files rather than a
download link.

**Local media without an account.** `local_media.rs` reads the Windows GSMTC
session, so play/pause/skip/seek and artwork work with no Spotify login at all.

---

## Design system

**Liquid Glass**, tuned for a dark desktop.

| Token | Value |
| --- | --- |
| Text | `#FFFFFF` / `#A1A1AA` / `#6E6E78` |
| Accent | `#1ED760`, only on active affordances |

There is deliberately **no native DWM backdrop**. `Effect::Acrylic` maps to
`DWMWA_SYSTEMBACKDROP_TYPE`, which is applied to the whole HWND rectangle and
cannot be clipped by CSS `border-radius` — it produced a visible square panel behind
the rounded pill. It is also a no-op here: a layered window (`WS_EX_LAYERED`, which
is what makes transparency work) does not accept DWM system backdrops.

So every optical cue is painted in CSS, clipped by the shell's own radius:

1. **Tint** — a translucent dark fill whose alpha is driven by the *Transparency*
   setting, plus one gentle vertical ramp.
2. **Album ambient** — two very wide, very faint radial pools coloured from the
   current artwork, desaturated so a saturated cover never reads as a neon sign.
3. **Specular sheen** — a diagonal band that leans toward the pointer.
4. **Cursor light** — a radial reflection following the pointer, written to
   `--gx`/`--gy`/`--gi` once per frame instead of through React state.
5. **Rim light** — a 1px hairline drawn with `mask-composite` so the highlight
   never tints the fill.

All shadows are **inset**. A drop shadow paints outside the border box, and the
window is exactly the shell's size, so the OS clips it at the window edge and leaves
a hard rectangular ring in the corners that are meant to be transparent.

Blur is capped at 16 px (`blurToPx`). A large `backdrop-filter` over a transparent
window averages the desktop into one flat tone — measured: surface stdev 8 at 29 px
blur versus 35–51 at 2–10 px.

**Solid mode** swaps the material rather than adjusting it: opaque `#000`, no blur,
no album wash, no sheen, no cursor light, and the artwork stops being tinted. The
Transparency and Blur sliders keep their values underneath, so turning Solid off
restores exactly what you had.

No emoji, no raster icons, no cyber/Matrix clichés, no neon gradients. Every icon is
a Lucide stroke SVG or a hand-written path in `src/components/icons.tsx`.

---

## Performance

- **Local media poll**: 1 s while playing, 2 s while paused, 5 s with no session.
  A single `SessionManager` is reused instead of being rebuilt per tick.
- **Spotify poll**: 1 s playing, 3 s paused, 30 s disconnected. Nothing is polled
  without a token.
- **Hover watcher**: 30 Hz against the OS cursor, parked entirely while the feature
  is off or a non-mini mode is in force.
- **Progress bar**: interpolated at 4 Hz rather than every animation frame.
- **No idle rendering**: worker threads sleep; the webview renders on state change.
- **Backoff**: rate limits and network errors back off instead of retrying hot.
- **Single instance**: a second launch focuses the existing player.
- **Release profile**: `lto`, `codegen-units = 1`, `opt-level = "s"`, `strip`,
  `panic = "abort"`.

---

## Configuration files

| What | Where |
| --- | --- |
| Settings | `%APPDATA%\com.sonic.player\settings.json` |
| Window position | `%APPDATA%\com.sonic.player\geometry.json` |
| File Shelf | `%APPDATA%\com.sonic.player\shelf\` + `shelf.json` |
| Clipboard Vault | `%APPDATA%\com.sonic.player\clipboard\` + index |
| Refresh token | Windows Credential Manager, entry `Sonic` (DPAPI, per user) |

A `.env` file is read at startup if present — next to the executable in a release
build, or the project root in `tauri dev`. Values already in the environment win, so
a shell export or CI variable overrides the file. See `.env.example`.

---

## Project layout

```
src/
  components/
    AlbumArtwork/     artwork with crossfade + ambient glow
    Player/           pill, mini, expanded
    PlaybackControls/ transport with animated play/pause glyph
    ProgressBar/      scrubber with pointer capture
    Pocket/           PocketPanel, PocketStage (sliding page), DropTarget
    Settings/         in-window settings panel
    GlassSurface/     the interactive glass shell
    VolumePopover/    volume + device summary
    DevicePicker/     Spotify Connect device list
    ConnectionStatus/ every empty/error state
    Equalizer/        decorative "is playing" bars
    icons.tsx         Spotify mark, note, waveform
  hooks/
    useSonicEvents.ts        Tauri event bridge
    usePlaybackProgress.ts   interpolation, dismissable
    usePointerClaim.ts       tells Rust a drag/popover owns the pointer
    useHotkeys.ts
    useAlbumColors.ts        dominant colour for the ambient tint
    usePointerGlow.ts        cursor light, written as CSS variables
    useReducedMotion.ts
  lib/
    format.ts         time, artists, bytes, glass maths
    layout.ts         per-mode geometry tokens
    motion.ts         shared durations and springs
  services/spotify/api.ts    every Tauri command, typed
  store/playerStore.ts       authoritative snapshot + optimistic overrides
  styles/globals.css         glass tokens, layers, controls, sliders
  types/spotify.ts           mirrors of the Rust serde types
  App.tsx, main.tsx

src-tauri/src/
  lib.rs              builder, plugins, setup
  config.rs           settings, paths, autostart, window modes
  error.rs            typed AppError (serialises for the UI)
  vault.rs            Credential Manager + DPAPI fallback
  auth.rs             PKCE, loopback listener, code exchange
  spotify.rs          ureq client, token refresh, endpoint wrappers
  local_media.rs      Windows GSMTC session (no account required)
  state.rs            AppState, UiState
  player.rs           poller with backoff
  window.rs           frameless window, animated geometry, position memory
  hover.rs            pointer-hover mini↔pill collapse (OS-cursor hit test)
  drag_drop.rs        native file drag-in
  pocket.rs           Pocket commands + native OLE drag-out
  shelf.rs            File Shelf index and copy
  clipboard.rs        Clipboard Vault: capture, filtering, restore
  clipboard_sys.rs    Win32 clipboard access
  clipboard_watch.rs  clipboard change polling
  tray.rs             tray icon + context menu
  commands.rs         the Tauri command surface
```

---

## Credits

Built with Tauri 2, React 18, TypeScript, Tailwind CSS, Framer Motion, Lucide and
Zustand.

Spotify integration uses the official Spotify Web API. Not affiliated with,
endorsed by, or sponsored by Spotify. "Spotify" and the Spotify logo are trademarks
of Spotify AB.

---

## License

MIT — see [LICENSE](./LICENSE).