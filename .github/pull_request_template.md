## What and why
<!-- One logical change. Link the issue or plan item (docs/vX.Y.Z.md). -->

## Checks
- [ ] `cargo fmt --check`
- [ ] `cargo clippy --locked --all-targets -- -D warnings`
- [ ] `KFXL_NO_UPDATE_CHECK=1 cargo test --locked`
- [ ] `CHANGELOG.md` → `Unreleased` (if user-visible)
- [ ] EN + UK strings / manual (if UI changed)
- [ ] Manual audio / plugin check (if engine, audio, plugins or `.kfxl` changed): OS, backend, plugin, rate, buffer, result

## Not verified
<!-- Windows runtime, real WASAPI/ASIO, live plugins, etc. -->

## Author
<!-- Owner, or agent + model -->
