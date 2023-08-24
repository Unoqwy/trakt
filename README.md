# trakt

Reliable reverse proxy and load balancer for Minecraft: Bedrock Edition servers.

**WARNING: This is brand new. Reliability/performance claims may not yet be true.**

## Features

- Efficient Raknet-aware proxying
- MOTD Caching
- Proxy Protocol support to forward player IPs
- Health checks (ping) to not send players to servers that are down
- Dynamic configuration reload

## Installation

### From source

Make sure you have a recent version of the rust toolchain installed.

1. Clone this repository: `git clone https://github.com/Unoqwy/trakt`
2. Go into the cloned directory: `cd trakt`
3. Build and install the binary.
   1. As a cargo binary: `cargo install --locked --path .`
   2. (or) system-wide: `cargo build --release --locked && sudo cp target/release/trakt /usr/local/bin/trakt`

## Usage

```
Usage: trakt [OPTIONS]

Options:
  -c, --config <FILE>  Configuration file
  -v, --verbose...     Verbose level
      --ignore-stdin   Disable reading from standard input for commands
      --no-color       Disables colors from output
  -h, --help           Print help
  -V, --version        Print version
```

To create the config file, it's recommend to copy [config.example.toml](./config.example.toml) from this repository. You can then edit it to fit your needs.

### Reloading

The configuration can be reloaded without restarting trakt. To do so, type `reload` in the program's console.

### As a systemd service

A sample systemd service file is provided in [pkg/trakt.service](./pkg/trakt.service).

You can use `sudo systemctl link $(realpath pkg/trakt.service)` to link it.

Before linking it, make sure the command in `ExecStart` matches your installation. By default, you need the following:

- Binary installed in `/usr/local/bin/trakt`
- Config file in `/etc/trakt.toml`

Note: There is currently no practical way to reload the config when running trakt a systemd service.
