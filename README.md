# Discombobulator

Fast, keyboard-first photo culling for Fujifilm RAF + JPG on Windows, with an iPhone companion over
Tailscale.

- Desktop: decide burst by burst with one key per decision (Stacks view).
- Phone: swipe through the inbox from the sofa, online or offline (Swipe view).
- Lightroom: picks, rejects, ratings and labels go to standard XMP sidecars.
- Nothing is ever erased: Delete moves files to `_to_delete/`.

See [SPEC.md](SPEC.md) for the full behavior and [CLAUDE.md](CLAUDE.md) for how the repo works.

## Install

Download the latest `Discombobulator_x.y.z_x64-setup.exe` from
[Releases](https://github.com/Thornolf/discombobulator/releases) and run it.

## Develop

```sh
npm --prefix web ci
npm --prefix web run build
cargo run -p discombobulator-server -- serve
```

## License

[MIT](LICENSE)
