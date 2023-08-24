# trakt

Reliable reverse proxy and load balancer for Minecraft: Bedrock Edition servers.

**WARNING: This is brand new. Reliability/performance claims may not yet be true.**

## Features

* Efficient Raknet-aware proxying
* MOTD caching
* Proxy Protocol support to forward player IPs
* Health checks (ping) to not send players to servers that are down

## Usage

You can get trakt up and running from this point in a few minutes.

1. Compile the binary
2. Copy config.example.toml into a new file (e.g. config.toml)
3. Modify the config to your liking
4. Run `trakt --config <file>` (e.g. `track --config config.toml`)

