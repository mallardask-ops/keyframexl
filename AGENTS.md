# KeyFrameXL — agent rules

Native pattern-based DAW in Rust (egui 0.31, cpal 0.18) for Linux and Windows.
Full process, with reasons: [docs/development.md](docs/development.md) (Ukrainian).
Owner speaks Ukrainian; answer in Ukrainian unless asked otherwise. Code, commits,
CHANGELOG: English.

## Branches and worktrees
- Never commit or push to `main`. Work on your own branch in your own worktree:
  `t3/<slug>` (created by T3) or `agent/<model>/<slug>`.
- One task = one branch = one worktree. Do not touch other agents' worktrees.
- Changes land via PR, squash-merged, after `linux-test` and `windows-test` pass.

## Commits
- Conventional Commits, imperative, ≤72 chars: `type(scope): Summary`.
  Types: feat fix perf refactor test docs build ci chore revert.
  Scopes: engine audio fx synth sampler mixer roll playlist rack ui i18n plugins bridge
  vst3 clap wine flp midi export project vocal forge updater platform release vendor.
- One logical change per PR. Never bundle unrelated fixes.
- `!` after scope for breaking `.kfxl`/settings changes; describe migration in the body.
- End agent commits with a `Co-Authored-By:` trailer for your model.

## Code
- Platform `#[cfg]` only in `src/platform/`, `src/bridge/ipc/`,
  `src/plugins/editor_*.rs`, `src/audio.rs`; never inside `src/ui/` or DSP.
- Do not grow `engine.rs`, `ui/roll.rs`, `ui/mod.rs`, `flp.rs`: put new logic in submodules;
  split in a separate `refactor:` commit first.
- New crate: justify in the PR (features, native libs, license MIT/Apache/BSD/Zlib/ISC).
  Vendored changes must be recorded in `vendor/README.md`.
- Do not enable `cpal/realtime-dbus` (kills Wine/yabridge children via RLIMIT_RTTIME).
- User-visible strings go through i18n with EN and UK; follow `docs/ui-rules.md` for UI.

## Before handing off a PR
```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
KFXL_NO_UPDATE_CHECK=1 cargo test --locked
```
- User-visible change: add 1–2 lines under `## Unreleased` in `CHANGELOG.md`
  (Added/Changed/Fixed/Removed), written for a musician, not about internals.
- Audio/engine/plugin/format change: do the manual check in docs/development.md §6 and
  report OS, backend, plugin, sample rate, buffer, result.
- PR description: what, why, commands run, what was NOT verified.

## Shared machine
- Default tests must not touch the audio device, `~/.wine`, or the network.
- Audio device, `~/.wine`/yabridge, FL Link, `scripts/live/*`: one agent at a time, via
  `flock /tmp/kfxl-hw.lock <cmd>`. Use a scratch `WINEPREFIX` for plugin installs. Never kill
  other agents' Wine or PipeWire processes.
- Renders, logs, screenshots go to `/tmp/kfxl-*` or `artifacts/<branch>/`, never repo root.

## Versions and releases
- Version lives only in `Cargo.toml` (`X.Y.Z-dev` between releases). Do not bump it, tag,
  or publish releases unless the owner asks. Release steps: docs/development.md §5.
