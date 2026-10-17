#!/usr/bin/env bash
# Builds the committed Faber on a Raspberry Pi and (re)starts it there. Runs on your computer
# (Git Bash on Windows works) with SSH access to the Pi:
#
#   tools/pi/deploy.sh [user@host]        default: mikkel@sennep.local
#   tools/pi/deploy.sh --setup [user@host] first time: also runs tools/pi/setup.sh on the Pi
#
# The shell is built here (the Pi needs no Node.js); the core is built on the Pi with the shell
# compiled in. Only committed files are sent, so what runs on the Pi is always a known commit.
set -euo pipefail

setup=0
if [ "${1:-}" = "--setup" ]; then
    setup=1
    shift
fi
pi="${1:-mikkel@sennep.local}"
repo="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo"

if [ -n "$(git status --porcelain --untracked-files=no)" ]; then
    echo "deploy.sh: uncommitted changes are not sent; deploying $(git rev-parse --short HEAD) as committed." >&2
fi

echo "== Building the shell"
npm --prefix shell run build

echo "== Sending $(git rev-parse --short HEAD) to $pi"
ssh "$pi" 'rm -rf ~/faber/src && mkdir -p ~/faber/src'
git archive HEAD | ssh "$pi" 'tar -x -C ~/faber/src'
tar -C shell -c dist | ssh "$pi" 'tar -x -C ~/faber/src/shell'

if [ "$setup" = 1 ]; then
    echo "== Setting up the Pi"
    ssh "$pi" 'bash ~/faber/src/tools/pi/setup.sh'
fi

echo "== Building faber-core on the Pi"
ssh "$pi" 'cd ~/faber/src && CARGO_TARGET_DIR=~/faber/target ~/.cargo/bin/cargo build --release -p faber-core --features embed-shell'
ssh "$pi" 'sudo install -m 755 ~/faber/target/release/faber-core /usr/local/bin/faber-core && sudo systemctl restart faber-core'

echo "deploy.sh: faber-core is running on $pi; open http://${pi#*@}:7878"
