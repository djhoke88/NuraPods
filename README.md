# NuraPods

NuraPods is an Ubuntu system-tray app for AirPods battery status, backed by a small Rust daemon.

## MVP

- continuously watches Apple Bluetooth LE advertisements and the AirPods L2CAP metadata channel;
- falls back to standard BlueZ LE discovery when its experimental advertisement-monitor API is unavailable;
- reads left, right, and case battery levels when available;
- exposes the aggregate battery through BlueZ and component details to the tray;
- runs as a user systemd service, with the tray app started at login.

The Bluetooth backend is adapted from the local `airpodsd` project. The unencrypted BLE battery parser is adapted from `podpower`. AirPods metadata is a reverse-engineered protocol and support varies by model and firmware. Audio playback and noise-control commands are outside this first MVP; Ubuntu's existing Bluetooth and audio stack continues to manage audio.

The BLE-only fallback reports directly to the tray. BlueZ battery publication is used for devices handled through the authenticated daemon path.

## Build and install

Requirements: Ubuntu with BlueZ, a BLE-capable Bluetooth adapter, Rust stable, `pkg-config`, `libbluetooth-dev`, `libgtk-3-dev`, and `libayatana-appindicator3-dev`. Install the native build dependencies with:

```sh
sudo apt install pkg-config libbluetooth-dev libgtk-3-dev libayatana-appindicator3-dev
```

On GNOME, enable the Ubuntu AppIndicator extension to show the tray icon.

```sh
./install.sh
```

The installer builds both binaries, installs the daemon as `~/.config/systemd/user/nurapods-daemon.service`, enables it, and adds the tray app to desktop autostart. Start the tray immediately with:

```sh
~/.local/bin/nurapods-tray
```

Useful commands:

```sh
systemctl --user status nurapods-daemon
journalctl --user -u nurapods-daemon -f
~/.local/bin/nurapods-daemon status --json
```

The tray refreshes every three seconds. It reads a machine-readable status response from the daemon over a local Unix socket. Its menu includes an explicit refresh and quit action.

## Project layout

- `daemon/`: Bluetooth discovery, Apple protocol handling, BlueZ battery provider, and local status socket.
- `tray/`: StatusNotifierItem app and battery menu.
