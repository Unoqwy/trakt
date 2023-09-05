# trakt

Reliable reverse proxy and load balancer for Minecraft: Bedrock Edition servers.

**Development branch! Things will change and break.** Stay on master branch for now.

## Features

- Efficient Raknet-aware proxying
- MOTD Caching
- Proxy Protocol support to forward player IPs
- Health checks (ping) to not send players to servers that are down
- Dynamic configuration reload
- Ability to restart and recover active connections (provided it restarts within a few seconds)
- HTTP API
- Web dashboard
- Low resources usage

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

To reload the configuration without restart, you can:

- Type `reload` in the console
- Click a button on the web-based dashboard (WIP)

Note: By using the HTTP API and/or the dashboard, several extra features are available that can remove the need to edit configurations.

### HTTP API

The HTTP API is documented following OpenAPI 3.0 specifications.

With the API server up and running, navigate to `/v0/openapi.json` (e.g. `http://localhost:8084/v0/openapi.json`) to find the specifications. [Swagger UI](https://swagger.io/tools/swagger-ui/) is also available by default at `/v0/swagger-ui`.

### As a library

If you need a custom load balancer to integrate with the rest of your infrastructure, this project can be used as a library to build on top of.  
However, trakt aims to cover most cases and expose advanced features with the HTTP API. If you implement a feature that can be useful to others, consider contributing.

Your starting point should be adding `trakt_core` as a dependency. Then, you can look at the documentation, and check out how trakt itself uses the library [here](./src).
