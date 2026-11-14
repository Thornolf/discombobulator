# Discombobulator specification

Fast, keyboard-first photo culling for Fujifilm RAF + JPG on Windows, with an iPhone companion over
Tailscale. Personal project, MIT licensed.

## Goals

1. Decide on thousands of photos quickly: one key per decision, no waiting between photos.
2. Never lose a photo: nothing is ever erased; Delete moves files to `_to_delete/`.
3. Hand decisions to Lightroom Classic through standard XMP sidecars.
4. Cull from the sofa on an iPhone, online or offline.

## Platforms

- Desktop: Windows 10/11, a Tauri window, one-click installer, starts with Windows.
- Phone: iOS Safari, installable PWA (Add to Home Screen, full screen), reached through
  `tailscale serve` (HTTPS on the tailnet). The phone downloads previews only, never RAFs.
- One Svelte + TypeScript UI codebase with two shells: desktop **Stacks** and phone **Swipe**.

## Architecture

```
crates/core     scan, pairing, RAF/EXIF/XMP parsing, thumbnail + preview cache, SQLite,
                scenes/stacks grouping, duplicates, XMP sidecar writing, apply (moves)
crates/server   axum HTTP API + embedded web app, binds 127.0.0.1 only
crates/desktop  Tauri shell: starts the server in-process, opens its page, autostart, single instance
web             Svelte + TS app, shared state and API client, two shells
```

The server only listens on `127.0.0.1`. The phone reaches it through `tailscale serve`, which proxies
HTTPS from the tailnet to localhost, so the app is never exposed on the LAN or the internet.

### Photo model

- A photo is one capture: a RAF, a JPG, or a RAF + JPG pair with the same stem in the same folder
  (case-insensitive extension: `.raf`, `.jpg`, `.jpeg`). The pair moves together.
- Display image: the JPG when present, otherwise the JPEG embedded in the RAF.
- RAF embedded JPEG: file starts with the magic `FUJIFILMCCD-RAW `; the JPEG offset is a big-endian
  `u32` at byte 84 and its length a big-endian `u32` at byte 88. Bounds are validated against the file
  size and the slice must start with the JPEG SOI marker `FF D8`.
- Capture time: EXIF `DateTimeOriginal` (plus `SubSecTimeOriginal` when present) read from the display
  JPEG; file modified time only as a fallback.
- Orientation: EXIF `Orientation` from the display JPEG is applied to thumbnails and previews.
- In-camera rating: `xmp:Rating` from XMP embedded in the RAF or the JPG, used as the initial rating
  when the photo has no rating of its own yet.

### Cache

- Stored under the local app data folder (`%LOCALAPPDATA%\discombobulator` on Windows).
- Thumbnail: long edge 360 px, JPEG. Preview: long edge 2560 px, JPEG (never upscaled).
- 64-bit dHash computed from the thumbnail.
- SQLite holds the photo index (paths, size, modified time, capture time, hashes, camera rating) and the
  decisions (mark, favorite, rating). A file whose size and modified time are unchanged is not
  re-processed, so rescans are near instant.
- Preload builds every thumbnail, preview and hash in parallel right after a folder is opened.

## Marks and metadata

| Action | Keys | Lightroom (XMP sidecar) |
|---|---|---|
| LR approved | `A`, `Space`, `L` | `xmpDM:pick="1"`, `xmpDM:good="true"` |
| Delete | `X`, `Delete` | `xmpDM:pick="-1"`, `xmpDM:good="false"`; on Apply the files move to `_to_delete/` |
| Unsure | `U` | `xmp:Label="Yellow"` |
| Favorite | `F` | `xmp:Label="Purple"` unless Unsure |
| Rating 1–5 | `1`–`5`; same number again clears; `0` clears | `xmp:Rating` |
| Clear mark | `Backspace` | removes pick/good/label written by the app |
| Undo | `Ctrl+Z` | |

- Approve, Delete and Unsure are exclusive; pressing the same mark key again toggles it off.
- Favorite and rating are independent of the mark.
- Approve, Delete and Unsure auto-advance (only when they set a mark, not when they toggle it off);
  Favorite and rating do not.
- Undo reverts the last change, including every frame changed by one Keep.
- LrC 13.2+ reads pick flags from XMP.

### XMP sidecars

- Written on Apply, for every photo with a mark, favorite or rating, as `<stem>.xmp` next to the RAF
  (next to the JPG when there is no RAF).
- An existing sidecar is preserved: only `xmp:Rating`, `xmp:Label`, `xmpDM:pick` and `xmpDM:good` are
  replaced; everything else (develop settings, keywords) is kept byte for byte where possible.
- The write goes to a temporary file in the same folder, then is renamed over the target.

### Apply

1. Write XMP sidecars.
2. Move every Delete photo (RAF, JPG and sidecar) to `<photo folder>/_to_delete/`.
- Never deletes, never overwrites: on a name clash the file gets a ` (n)` suffix.
- Refuses symlinks and any path that resolves outside the opened root.
- Reports what moved and what failed; a failure on one photo does not stop the others.

### Empty _to_delete

- After checking `_to_delete`, the user can send its files to the operating system's Recycle Bin
  (Trash on macOS/Linux) from the Apply dialog, after a confirmation showing the file count and size.
- Only regular files inside `_to_delete` folders under the open folder; links are never followed.
- Files go to the Recycle Bin, never erased; a failure is reported per file and never falls back to a
  permanent delete.

## Grouping

- Photos sorted by capture time, then path.
- **Scene**: a new scene starts after a gap of more than 5 minutes.
- **Stack** (burst): consecutive shots ≤ 2.5 s apart, or dHash distance ≤ 12 to the previous frame.
- Phone "Looks like the previous one" badge: dHash distance ≤ 10 to the previous card.
- All thresholds are settings.

## Desktop UI: Stacks

- Header: open folder, position (`Scene 3/7 · stack 12/40`), Next undecided, Duplicates, Apply, help.
- Left: scenes, each with a mosaic, photo count, progress bar (approved/unsure/delete), stacks count and
  "N to decide" or "done".
- Center: the current stack side by side, a header with BURST × n or SINGLE, time and span, and the
  Keep button for bursts. Each frame shows mark pills, favorite heart, clickable stars and file name.
- Bottom: strip of the scene's stacks with per-frame status dots, ×n badge and best rating.

| Keys | Action |
|---|---|
| `←` `→` | Focus frame; past the end moves to the next/previous stack |
| `↑` `↓` | Previous / next stack |
| `PgUp` `PgDn` | Previous / next scene |
| `Enter` | Solo view of the focused frame; `Esc` leaves it |
| `N` | Next stack with undecided photos |
| `E` | Apply |
| `S`, Ctrl/Shift+click | Select / unselect the focused frame |
| `K` | Keep |
| `D` | Duplicates view |
| `O` | Open folder |
| `?`, `H` | Help |

- Marking a frame inside a burst moves focus to the next frame; after the last frame, to the next stack.
- Keep: the selected frames (or the focused one) become approved; only still-undecided frames in the
  stack become Delete. Frames already approved or Unsure are never touched. Approve alone never affects
  other frames.

## Phone UI: Swipe

- One card at a time from the inbox (unmarked photos), next card peeking behind.
- Swipe or arrows: left Delete, right approve, up favorite + approve, down Unsure. Buttons for the same,
  plus undo. Double tap zooms.
- Badge "Looks like the previous one" when the dHash is close.
- End screen: totals, "Re-check unsure" pass, "Review piles" (grid per pile: Delete, Unsure, Approved,
  Favorites; change a photo or send it back to the inbox), Apply.
- Offline: "Download for offline" stores the folder's photo list and previews on the phone; marks made
  offline are queued with the folder they belong to and synced when the PC has that folder open.
  Home-screen web apps on iOS are exempt from Safari's 7-day storage eviction. "Remove offline copy"
  frees the space. Only the phone shell installs the service worker; the desktop never caches photos.
- Sending a photo back to the inbox clears its mark and keeps its favorite.

## Keyboard layouts

- Letter shortcuts match the produced character (`KeyboardEvent.key`, case-insensitive).
- Digits match the physical key (`KeyboardEvent.code` `Digit0`–`Digit5`, `Numpad0`–`Numpad5`), so
  AZERTY needs no Shift.
- No default shortcut needs AltGr on AZERTY (`[` `]` `{` `}` `\` `@` `#`).
- Every shortcut is remappable; the keymap is stored per device.

## Duplicates

- Exact: same size, then BLAKE3 of the first and last 64 KiB, then full BLAKE3 to confirm. Hashes are
  cached.
- Similar: dHash within a tolerance (slider, default 6), found with a BK-tree.
- View: groups (exact first), keep one with `K`: the others become Delete.

## Speed

- Never decode RAW for display: slice the embedded JPEG.
- Preload everything after opening a folder; parallel decoding across all cores.
- Keep the next ~5 and previous ~2 previews decoded around the cursor.
- Virtualized lists for tens of thousands of photos.

## Security

- HTTP server binds `127.0.0.1` only; phone access only through `tailscale serve`.
- Requests whose `Host` is not `localhost`, `127.0.0.1` or a `*.ts.net` name are rejected (DNS
  rebinding).
- State-changing requests need the `X-Discombobulator: 1` header and, when an `Origin` is sent, an
  origin matching the `Host` header or the PC's own tailnet machine name (blocks cross-site requests
  from web pages). `X-Forwarded-Host` is only checked against the host allowlist, never trusted to
  vouch for an origin.
- Photo ids are opaque; the API never takes a file path except to open a folder, and that path must be
  an existing directory.
- Browsing and opening folders is reserved to the PC: `GET /api/folders` and `POST /api/library` answer
  403 to requests that came through `tailscale serve` (a `*.ts.net` `Host` or `X-Forwarded-Host`, or a
  `Tailscale-User-Login` header), so other tailnet devices never see the PC's folder tree. The phone
  culls the folder the PC has open; `/api/state` reports `can_open_folders` so the phone hides the
  folder picker.
- The app never deletes a user file and never follows symlinks when moving.
- RAF, JPEG, EXIF and XMP parsing is bounds-checked and never panics on malformed input.

## Release

- Conventional Commits on `main`; release-please opens a release PR with the changelog and version bump.
- Merging the release PR tags `vX.Y.Z`, creates the GitHub release and builds the Windows NSIS
  installer, attached to that release.

## Prior art

Photo Mechanic, FastRawViewer, Narrative Select, Excire Foto, Czkawka.
