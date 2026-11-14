# Discombobulator

Fast, keyboard-first photo culling for Fujifilm RAF + JPG on Windows, with an iPhone companion over
Tailscale.

- **Desktop (Stacks)**: photos are grouped into scenes and bursts; decide one burst at a time with one key
  per decision.
- **Phone (Swipe)**: swipe through the inbox from the sofa, online or offline.
- **Lightroom**: picks, rejects, stars and labels go to standard XMP sidecars.
- **Nothing is ever erased**: Delete moves files to a `_to_delete` folder next to them.

[SPEC.md](SPEC.md) describes every behavior; [CLAUDE.md](CLAUDE.md) describes how the repository works.

## Install

1. Download `Discombobulator_x.y.z_x64-setup.exe` from the latest
   [release](https://github.com/Thornolf/discombobulator/releases).
2. Run it. It installs for your user only (no administrator rights) and adds a Start menu entry. The
   installer is not code-signed yet, so Windows SmartScreen may ask you to confirm with
   **More info → Run anyway**.
3. Discombobulator starts with Windows in the tray. Closing the window keeps it running so your phone can
   reach it; use **Quit** in the tray menu to stop it, and untick **Start with Windows** there to turn
   autostart off.

## Use

1. **Open folder** and pick a folder of RAF and/or JPG files. Thumbnails and previews are built in the
   background and cached, so the next visit is instant.
2. Decide with the keyboard (press `?` for the full list, every key is remappable):

   | Key | Action |
   |---|---|
   | `A` / `Space` | LR approved |
   | `X` / `Del` | Delete |
   | `U` | Unsure |
   | `F` | Favorite |
   | `1`–`5`, `0` | Stars, clear stars |
   | `S`, `K` | Select frames in a burst, keep them and delete the undecided rest |
   | `←` `→` `↑` `↓` | Frame, stack |
   | `N` | Next undecided stack |
   | `D` | Duplicates |
   | `E` | Apply |
   | `Ctrl+Z` | Undo |

3. **Apply** writes the XMP sidecars and moves Delete photos to `_to_delete`. In Lightroom Classic,
   import the folder (or use **Metadata → Read Metadata from Files**) to get the picks, rejects, stars and
   labels (Unsure = Yellow, Favorite = Purple).
4. Once you have checked `_to_delete`, **Empty into Recycle Bin…** in the Apply dialog sends those files to
   the Windows Recycle Bin, where they can still be restored.

## Phone

1. Install [Tailscale](https://tailscale.com) on the PC and the iPhone, signed in to the same account.
2. In Discombobulator click **Phone → Enable phone access** and scan the QR code.
3. In Safari tap **Share → Add to Home Screen**, then open **Cull** from the Home Screen.
4. For culling without the PC, open the menu and **Download this folder for offline**. Marks sync back
   when the PC is reachable and has that same folder open.

The app listens on `127.0.0.1` only; the phone reaches it through `tailscale serve`, so it is never exposed
on your network or the internet.

## Develop

```sh
cargo run --release -p discombobulator-server -- bench --fresh "D:\Photos\2026-10-12"
npm --prefix web ci
npm --prefix web run build
cargo run -p discombobulator-server -- serve
npm --prefix web run dev
```

Releases are automated: merging the release PR that release-please opens tags the version and attaches the
Windows installer to the GitHub release.

## License

[MIT](LICENSE)

Preview encoding uses the `jpeg-encoder` crate, which is based in part on the work of the Independent
JPEG Group.
