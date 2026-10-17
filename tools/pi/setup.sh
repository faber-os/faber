#!/usr/bin/env bash
# Prepares a fresh Raspberry Pi OS (64-bit) for faber-core. Runs on the Pi, as the user that will
# run the core, and is safe to run again. tools/pi/deploy.sh copies it over and runs it.
#
# What it does: installs the build tools, installs Rust for this user, turns on I²C and SPI, gives
# the GPIO UART (pins 8/10) to programs instead of the login console, and installs the
# faber-core service (it starts once a binary is installed).
set -euo pipefail

if [ "$(id -u)" -eq 0 ]; then
    echo "setup.sh: run this as your own user, not root (it uses sudo where it must)." >&2
    exit 1
fi
here="$(cd "$(dirname "$0")" && pwd)"

echo "== Packages"
sudo apt-get update
sudo apt-get install -y build-essential pkg-config libudev-dev git i2c-tools

echo "== Rust"
if ! command -v cargo >/dev/null && [ ! -x "$HOME/.cargo/bin/cargo" ]; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
fi
"$HOME/.cargo/bin/rustup" update stable

echo "== Interfaces"
# raspi-config's nonint calls take 0 for "on" and 1 for "off".
sudo raspi-config nonint do_i2c 0
sudo raspi-config nonint do_spi 0
sudo raspi-config nonint do_serial_cons 1 # no login console on the UART
sudo raspi-config nonint do_serial_hw 0   # but the UART itself stays on, for the Arduino
sudo usermod -aG dialout,i2c,spi,gpio "$USER"

echo "== Service"
sudo install -d -o "$USER" -g "$USER" /var/lib/faber
sed "s/@USER@/$USER/" "$here/faber-core.service" | sudo tee /etc/systemd/system/faber-core.service >/dev/null
sudo systemctl daemon-reload
sudo systemctl enable faber-core.service

echo "setup.sh: done. Reboot once so the interface changes take effect."
