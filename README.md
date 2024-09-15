cargo-llvm
=========

[![crate](https://img.shields.io/crates/v/cargo-llvm.svg)](https://crates.io/crates/cargo-llvm)
[![docs.rs](https://docs.rs/cargo-llvm/badge.svg)](https://docs.rs/cargo-llvm)

Manage multiple LLVM/Clang build

Why
-------
The original crate is no longer maintained, and I needed a way to manage multiple LLVM/Clang builds for my projects. This fork is intended to be a drop-in replacement for the original crate, with some additional features and bug fixes.

You can find original repo here: https://github.com/llvmenv/llvmenv and here https://github.com/llvmenv/llvmenv/issues/72

Install
-------

0. Install cmake, builder (make/ninja), and C++ compiler (g++/clang++)
1. Install Rust using [rustup](https://github.com/rust-lang-nursery/rustup.rs) or any other method.
2. `cargo install cargo-llvm`

### Basic Usage

To install a specific version of LLVM after following the installation steps above, run these shell commands ("10.0.0" can be replaced with any other version found with `cargo-llvm entries`):

```
cargo-llvm init
cargo-llvm entries
cargo-llvm build-entry 10.0.0
```

zsh integration
-----

You can swtich LLVM/Clang builds automatically using zsh precmd-hook. Please add a line into your `.zshrc`:

```
source <(cargo-llvm zsh)
```

If `$LLVMENV_RUST_BINDING` environmental value is non-zero, cargo-llvm exports `LLVM_SYS_60_PREFIX=$(cargo-llvm prefix)` in addition to `$PATH`.

```
export LLVMENV_RUST_BINDING=1
source <(cargo-llvm zsh)
```

This is useful for [llvm-sys.rs](https://github.com/tari/llvm-sys.rs) users. Be sure that this env value will not be unset by cargo-llvm, only overwrite.

Concepts
=========

entry
------

- **entry** describes how to compile LLVM/Clang
- Two types of entries
  - *Remote*: Download LLVM from Git/SVN repository or Tar archive, and then build
  - *Local*: Build locally cloned LLVM source
- See [the module document](https://docs.rs/cargo-llvm/*/cargo-llvm/entry/index.html) for detail

build
------

- **build** is a directory where compiled executables (e.g. clang) and libraries are installed.
- They are compiled by `cargo-llvm build-entry`, and placed at `$XDG_DATA_HOME/cargo-llvm` (usually `$HOME/.local/share/cargo-llvm`).
- There is a special build, "system", which uses system's executables.

global/local prefix
--------------------

- `cargo-llvm prefix` returns the path of the current build (e.g. `$XDG_DATA_HOME/cargo-llvm/llvm-dev`, or `/usr` for system build).
- `cargo-llvm global [name]` sets default build, and `cargo-llvm local [name]` sets directory-local build by creating `.cargo-llvm` text file.
- You can confirm which `.cargo-llvm` sets the current prefix by `cargo-llvm prefix -v`.
