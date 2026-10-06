#!/usr/bin/env bash
set -euo pipefail

project_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cargo build --release --manifest-path "$project_dir/Cargo.toml"

install -d "$HOME/.local/bin" "$HOME/.local/share/nurapods" "$HOME/.config/systemd/user" "$HOME/.config/autostart"
install -m 0755 "$project_dir/target/release/nurapods-daemon" "$HOME/.local/bin/nurapods-daemon"
install -m 0755 "$project_dir/target/release/nurapods-tray" "$HOME/.local/bin/nurapods-tray"
install -m 0644 "$project_dir/daemon/nurapods-daemon.service" "$HOME/.config/systemd/user/nurapods-daemon.service"

cat > "$HOME/.config/autostart/nurapods-tray.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=NuraPods
Comment=AirPods battery status in the system tray
Exec=$HOME/.local/bin/nurapods-tray
Terminal=false
X-GNOME-Autostart-enabled=true
Categories=Utility;AudioVideo;
EOF

systemctl --user daemon-reload
systemctl --user enable --now nurapods-daemon.service
printf 'NuraPods installed. Log out and back in, or run ~/.local/bin/nurapods-tray now.\n'
