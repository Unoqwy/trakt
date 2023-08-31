# trakt

Reliable reverse proxy and load balancer for Minecraft: Bedrock Edition servers.

**WARNING: This is brand new. Reliability/performance claims may not yet be true.**

## Features

- Efficient Raknet-aware proxying
- MOTD Caching
- Proxy Protocol support to forward player IPs
- Health checks (ping) to not send players to servers that are down
- Dynamic configuration reload
- Ability to restart and recover active connections (provided it restarts within a few seconds)

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
  -c, --config <FILE>  Configuration file [default: config.toml]
  -v, --verbose...     Verbose level
      --ignore-stdin   Disable reading from standard input for commands
      --no-color       Disable colors from output
      --raise-ulimit   Raise the maximum number of open files allowed to avoid issues
      --recovery-snapshot-file <FILE>  Snapshot file for restart recovery [default: .trakt_recover]
  -h, --help           Print help (see more with '--help')
  -V, --version        Print version
```

To create the config file, it's recommended to copy [config.example.toml](./config.example.toml) from this repository. You can then edit it to fit your needs.

### Reloading

The configuration can be reloaded without restarting trakt. To do so, type `reload` in the program's console.

### As a library

If you need a custom load balancer to integrate with the rest of your infrastructure, this porject can be used as a library to build on top of.

Your starting point should be adding `trakt_core` as a dependency. You can then look at documentation, and check out how trakt itself uses the library [here](./src).

