# RTSyn

RTSyn is a Rust-based real-time electrophysiology platform for building and executing deterministic processing workflows. It features a modular architecture with plugins, connections, and workspaces, supporting both interactive GUI and headless daemon operation.

## Dependences

### Rust toolchain (stable) with Cargo

Install Rust via rustup:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Then ensure your environment is loaded:

```bash
source "$HOME/.cargo/env"
```

### libfontconfig and pkg-config

On Debian/Ubuntu:

```bash
sudo apt install libfontconfig1-dev pkg-config
```

On Fedora/RHEL/CentOS

```bash
sudo dnf install fontconfig-devel pkgconf-pkg-config
```

On Arch:

```bash
sudo pacman -Syu fontconfig pkgconf
```

### Comedi DAQ Plugin (optional)

If you plan to use the Comedi DAQ plugin, follow the install steps in `app_plugins/comedi_daq/README.md` or run:

```bash
bash scripts/install_comedi.bash
```

## Usage

### Build

Build the entire workspace:

```bash
cargo build --release
```

For development (faster compilation):

```bash
cargo build
```

### Interactive GUI (default)

```bash
cargo run -p rtsyn
```

### Headless Operation

Run daemon for specified duration:

```bash
cargo run -p rtsyn -- daemon --duration-seconds 60
```

Run with workspace:

```bash
cargo run -p rtsyn -- daemon --workspace my_workspace.json --duration-seconds 120
```

Run for specific tick count:

```bash
cargo run -p rtsyn -- run --no-gui --ticks 1000
```

### Real-time Performance

For real-time preempt-rt scheduling:

```bash
cargo build --release --features preempt_rt
sudo setcap cap_sys_nice=ep target/release/rtsyn
```

With comedi drivers:

```bash
cargo build --release --features "preempt_rt,comedi"
```

## Plugin development

Plugins can be developed either in-tree under `plugins/` or as independent repositories using the `rtsyn-plugin` crate. For external development:

1. Add `rtsyn-plugin` as a dependency in your `Cargo.toml`
2. Implement the plugin interface and expose inputs/outputs
3. Add a `plugin.toml` manifest with name, kind, ports, and variables
4. Build and distribute as a standard Rust crate

In-tree plugins follow the same pattern but live directly in the `plugins/` directory and are added to the workspace `Cargo.toml`. Once built, plugins are discoverable in the GUI for installation and use.

## Tests

Run all tests in the workspace:

```bash
cargo test
```
