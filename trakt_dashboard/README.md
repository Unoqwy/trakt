# trakt - Dashboard

Web-based dashboard to manage and overview one or more trakt nodes.

## Usage

To install and start the dashboard, install the trakt binary and execute `trakt dashboard`.

Find out more about the main binary [here](../README.md).

### Configuration

The dashboard works by calling the REST API of one or more nodes.

```toml
[dashboard]
# Address to start the Web Dashboard server on.
bind = "0.0.0.0:8081"

# List of proxies and their REST API URLs.
proxies = [
  { name = "main", api_url =  "http://0.0.0.0:8084/v1" },
]

```

