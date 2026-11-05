# Continuous integration

`.github/workflows/ci.yml` runs on pull requests, pushes to `main`, and manual
dispatch. Superseded runs on the same ref are cancelled. Each job has a 25-minute
limit and caches Rust dependencies/build outputs by OS, architecture, toolchain,
feature set and `Cargo.lock`.

The matrix covers:

- Ubuntu x64: tests with `mac-client`, formatting and Clippy with warnings denied.
- Ubuntu ARM64: default/server tests and a native linked release build for the
  Pi's architecture (not a test on Dan's physical Pi).
- macOS ARM64: Mac-client tests and a native release build.

All jobs run both Python installer test modules using Python 3.12. Tests use
synthetic evidence, temporary SQLite databases and loopback MCP servers. No Pi,
private Gecko access, model key or service credentials are needed. The workflow
has read-only repository permissions, does not persist checkout credentials and
does not deploy, publish artifacts or install a system service. Action versions
are pinned to release commit SHAs.

Runner labels are listed in the [GitHub runner reference](https://docs.github.com/en/actions/reference/runners/github-hosted-runners).
Rust setup uses [rustup](https://rust-lang.github.io/rustup/installation/index.html)
and selects the installed stable toolchain directory directly. It supports
preinstalled/Homebrew rustup without requiring `~/.cargo/env`.

Local equivalents:

```sh
make test           # default/server Rust tests and both installer test modules
make test-all       # optional Mac-client Rust tests and both installer modules
cargo fmt --all -- --check
cargo clippy --locked --all-targets --features mac-client -- -D warnings
cargo build --locked --release --features mac-client
```

A locally validated workflow is not a hosted CI result. The first actual run
starts after the workflow is pushed to GitHub or included in a pull request.
