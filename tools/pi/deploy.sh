#!/usr/bin/env bash
# Builds the committed Faber on a Raspberry Pi and (re)starts it there. Runs on your computer
# (Git Bash on Windows works) with SSH access to the Pi:
#
#   tools/pi/deploy.sh user@host
#   tools/pi/deploy.sh --setup user@host   first time: also runs tools/pi/setup.sh on the Pi
#
# The shell is built here (the Pi needs no Node.js); the core is built on the Pi with the shell
# compiled in. Only committed files are sent, so what runs on the Pi is always a known commit.
set -euo pipefail

setup=0
if [ "${1:-}" = "--setup" ]; then
    setup=1
    shift
fi
if [ $# -ne 1 ]; then
    echo "usage: tools/pi/deploy.sh [--setup] user@host" >&2
    exit 2
fi
pi="$1"

# On Windows, use Windows' own OpenSSH: it talks to the Windows ssh-agent, so a key with a
# passphrase works without asking (Git Bash's ssh has no agent of its own). It can't read a Git
# Bash pipe, so files go over as one archive with scp, never piped through ssh.
if [ -x /c/Windows/System32/OpenSSH/ssh.exe ]; then
    ssh() { /c/Windows/System32/OpenSSH/ssh.exe "$@"; }
    scp() { /c/Windows/System32/OpenSSH/scp.exe "$@"; }
    native() { cygpath -w "$1"; }
else
    native() { echo "$1"; }
fi
repo="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo"

if [ -n "$(git status --porcelain --untracked-files=no)" ]; then
    echo "deploy.sh: uncommitted changes are not sent; deploying $(git rev-parse --short HEAD) as committed." >&2
fi

echo "== Building the shell"
npm --prefix shell run build

echo "== Sending $(git rev-parse --short HEAD) to $pi"
bundle="$(mktemp -d)"
trap 'rm -rf "$bundle"' EXIT
git archive --format=tar -o "$bundle/src.tar" HEAD
tar -C shell -cf "$bundle/dist.tar" dist
scp -q "$(native "$bundle/src.tar")" "$(native "$bundle/dist.tar")" "$pi:/tmp/"
ssh "$pi" 'rm -rf ~/faber/src && mkdir -p ~/faber/src && tar -xf /tmp/src.tar -C ~/faber/src && tar -xf /tmp/dist.tar -C ~/faber/src/shell && rm /tmp/src.tar /tmp/dist.tar'

if [ "$setup" = 1 ]; then
    echo "== Setting up the Pi"
    ssh "$pi" 'bash ~/faber/src/tools/pi/setup.sh'
fi

echo "== Building faber-core on the Pi"
ssh "$pi" 'cd ~/faber/src && CARGO_TARGET_DIR=~/faber/target ~/.cargo/bin/cargo build --release -p faber-core --features embed-shell'
ssh "$pi" 'sudo install -m 755 ~/faber/target/release/faber-core /usr/local/bin/faber-core && sudo systemctl restart faber-core'

echo "deploy.sh: faber-core is running on $pi; open http://${pi#*@}:7878"
