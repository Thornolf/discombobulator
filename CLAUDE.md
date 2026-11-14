# Working on Discombobulator

Fast, keyboard-first photo culling for Fujifilm RAF + JPG on Windows, with an iPhone companion over
Tailscale. `SPEC.md` is the source of truth for behavior. Read it before changing anything user-facing.

## Repository rules

- Everything in English: code, commits, docs, issues.
- Zero comments in code. Names and structure carry the meaning. This covers Rust, TypeScript, Svelte,
  CSS and config files that allow comments.
- Every merge to `main` goes through a pull request. Commits follow Conventional Commits
  (`feat:`, `fix:`, `chore:`, `docs:`, `ci:`, `refactor:`, `test:`, `perf:`), because release-please
  builds the changelog and the version from them.
- Two subagents review every PR before merge:
  - `code-reviewer` (`.claude/agents/code-reviewer.md`): correctness, simplicity, idiomatic Rust/Svelte,
    tests, adherence to SPEC.md and these rules.
  - `security-reviewer` (`.claude/agents/security-reviewer.md`): file system safety (never delete, path
    traversal, symlinks), network exposure (localhost + tailnet only), input parsing (RAF/XMP/EXIF),
    dependency risk.
- CI runs both reviews plus `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, `cargo deny`,
  `cargo audit`, and frontend lint, typecheck and tests. Branch protection on `main` requires all of them.
- More rules may be added by the owner later.

## Follow-ups and findings go in their own issues

Anything left for later gets its own GitHub issue before the work is reported as done: a known gap, a
limitation, a bug noticed along the way, an open question, a review finding not fixed in the PR. One
issue per finding, saying where it was found, what happens and the fix or the options. Attach it as a
sub-issue of the issue it comes from. A PR that finishes an issue says `Closes #N`.

## Layout

| Path | What |
|---|---|
| `crates/core` | Scan, RAF/EXIF/XMP parsing, cache, SQLite, grouping, duplicates, apply |
| `crates/server` | axum HTTP API + embedded Svelte app, binary `discombobulator` |
| `crates/desktop` | Tauri shell that runs the server in-process and opens its page |
| `web` | Svelte + TypeScript app, desktop shell (Stacks) and phone shell (Swipe) |

## Commands

```sh
npm --prefix web ci && npm --prefix web run build
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm --prefix web run lint && npm --prefix web run check && npm --prefix web test
cargo run -p discombobulator-server -- serve
cargo run -p discombobulator-server --release -- bench <folder>
```

Building `crates/desktop` on Linux needs `libwebkit2gtk-4.1-dev`, `libayatana-appindicator3-dev`,
`librsvg2-dev` and `libxdo-dev`.
