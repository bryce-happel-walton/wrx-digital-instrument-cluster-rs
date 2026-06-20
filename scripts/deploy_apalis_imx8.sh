#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/.."

APPLICATION_NAME="wrx-digital-instrument-cluster-rs"
TARGET="aarch64-unknown-linux-gnu"

# Usage: ./deploy_apalis_imx8.sh [profile] [extra,features] [-- binary args...]
# Anything after `--` is forwarded to the binary on the board.
POSITIONAL=()
BINARY_ARGS=()
while [[ $# -gt 0 ]]; do
    if [[ "$1" == "--" ]]; then
        shift
        BINARY_ARGS=("$@")
        break
    fi
    POSITIONAL+=("$1")
    shift
done

PROFILE="${POSITIONAL[0]:-debug}"
EXTRA_FEATURES="${POSITIONAL[1]:-}"

# Quote forwarded args so they survive the trip through the remote shell.
REMOTE_ARGS=""
for arg in "${BINARY_ARGS[@]}"; do
    REMOTE_ARGS+=" $(printf '%q' "$arg")"
done

case "$PROFILE" in
    debug)   PROFILE_DIR="debug" ;;
    release) PROFILE_DIR="release" ;;
    *)       PROFILE_DIR="$PROFILE" ;;
esac

"$SCRIPT_DIR/build_apalis_imx8.sh" "$PROFILE" "$EXTRA_FEATURES"

SSH_CONFIG="$SCRIPT_DIR/apalis_imx8_ssh_config"
BIN_PATH="target/$TARGET/$PROFILE_DIR/$APPLICATION_NAME"

printf "[deploy 1/4] Stopping weston and killing existing process on apalis-board...\n"
ssh -F "$SSH_CONFIG" -o StrictHostKeyChecking=accept-new apalis-board "systemctl stop weston || true; killall $APPLICATION_NAME || true" > /dev/null 2>&1

printf "[deploy 2/4] Copying binary to board...\n"
scp -F "$SSH_CONFIG" -o StrictHostKeyChecking=accept-new "$BIN_PATH" apalis-board:/bin/ 2>/dev/null

printf "[deploy 3/4] Verifying SHA256 hash...\n"
LOCAL_HASH=$(sha256sum "$BIN_PATH" | awk '{print $1}')
REMOTE_HASH=$(ssh -F "$SSH_CONFIG" -o StrictHostKeyChecking=accept-new apalis-board "sha256sum /bin/$APPLICATION_NAME | awk '{print \$1}'" 2>/dev/null)

if [[ "$LOCAL_HASH" != "$REMOTE_HASH" ]]; then
    printf "\tERROR: Hash mismatch after transfer!\n"
    printf "\tLocal : $LOCAL_HASH\n"
    printf "\tRemote: $REMOTE_HASH\n"
    exit 1
else
    printf "\tHash verified!\n"
fi

printf "[deploy 4/4] Starting new process on board...\n"

REMOTE_RUN="/bin/$APPLICATION_NAME$REMOTE_ARGS"
printf "\tLaunching: %s\n" "$REMOTE_RUN"

if [[ "$PROFILE" == "debug" ]]; then
    ssh -F "$SSH_CONFIG" -o StrictHostKeyChecking=accept-new apalis-board "export WAYLAND_DISPLAY=/run/wayland-0; $REMOTE_RUN"
else
    ssh -F "$SSH_CONFIG" -o StrictHostKeyChecking=accept-new apalis-board "export WAYLAND_DISPLAY=/run/wayland-0; nohup $REMOTE_RUN > /tmp/$APPLICATION_NAME.log 2>&1 &" > /dev/null 2>&1
fi

printf "\tDone! Deployed profile: $PROFILE\n"
