#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/.."

APPLICATION_NAME="wrx-digital-instrument-cluster-rs"
TARGET="aarch64-unknown-linux-gnu"

# Usage: ./deploy_apalis_imx8.sh [profile] [extra,features]
PROFILE="${1:-debug}"
EXTRA_FEATURES="${2:-}"

case "$PROFILE" in
    debug)   PROFILE_DIR="debug" ;;
    release) PROFILE_DIR="release" ;;
    *)       PROFILE_DIR="$PROFILE" ;;
esac

"$SCRIPT_DIR/build_apalis_imx8.sh" "$PROFILE" "$EXTRA_FEATURES"

SSH_CONFIG="$SCRIPT_DIR/apalis_imx8_ssh_config"
BIN_PATH="target/$TARGET/$PROFILE_DIR/$APPLICATION_NAME"

printf "[deploy 1/4] Killing existing process on apalis-board...\n"
ssh -F "$SSH_CONFIG" -o StrictHostKeyChecking=accept-new apalis-board "killall $APPLICATION_NAME || true" > /dev/null 2>&1

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

if [[ "$PROFILE" == "debug" ]]; then
    ssh -F "$SSH_CONFIG" -o StrictHostKeyChecking=accept-new apalis-board "export WAYLAND_DISPLAY=/run/wayland-0; /bin/$APPLICATION_NAME --cli"
else
    ssh -F "$SSH_CONFIG" -o StrictHostKeyChecking=accept-new apalis-board "export WAYLAND_DISPLAY=/run/wayland-0; nohup /bin/$APPLICATION_NAME --cli > /tmp/$APPLICATION_NAME.log 2>&1 &" > /dev/null 2>&1
fi

printf "\tDone! Deployed profile: $PROFILE\n"
