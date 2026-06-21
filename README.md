# MCSC

MCSC is a small Rust controller for Minecraft servers. It exposes a constrained
gRPC API for launching, stopping, backing up, downloading backups, and sending
allowlisted console commands without giving users SSH access to the host.

It is not a Minecraft installer. Set up the Minecraft server yourself, then
point MCSC at that server directory.

## Binaries

- `mcsc-server` runs the gRPC control service.
- `mcsc-cli` provides an interactive terminal client.
- `mcsc-gui` provides an iced desktop client.

## Security Model

Clients and the server share a long random secret in their config files. Each
request is signed with HMAC-SHA256 over the operation, request body, and a fresh
nonce. The server rejects invalid signatures and replayed nonces.

The command endpoint is additionally restricted by `allowed_commands` in
`mcsc_server.toml`. Only the first word of the console command is checked, so
keep the list narrow.

Use the default loopback socket for local use. If you bind `mcsc-server` to
`0.0.0.0`, put it behind a firewall, VPN, or similar network control.

## Configuration

Create matching secrets in both config files before running anything:

```toml
# mcsc_server.toml
socket = "127.0.0.1:7878"
minecraft_directory = "minecraft"
backup_directory = "backups"
key = "replace-with-a-long-random-secret-at-least-32-characters"
launch_command = ["sh", "launch.sh"]
allowed_commands = ["list", "say", "save-all", "whitelist"]
max_backups = 10
```

```toml
# mcsc_client.toml
ip = "http://127.0.0.1:7878"
key = "replace-with-the-same-long-random-secret"
```

`minecraft_directory` becomes the server process working directory. Backups are
stored in `backup_directory` relative to that directory.

## Building

MCSC vendors `protoc` for gRPC code generation, so a normal Rust toolchain is
enough:

```sh
cargo build
```

Run the server:

```sh
cargo run --bin mcsc-server
```

Run the CLI:

```sh
cargo run --bin mcsc-cli
```

Run the GUI:

```sh
cargo run --bin mcsc-gui
```

## Planned Features

- Permissions per user or key.
- Java version manager.
- Minecraft version manager.
- Mod downloader using Modrinth.
- Web UI.
