#!/usr/bin/env bash
set -Eeuo pipefail

PROJECT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
DAEMON="$PROJECT_DIR/target/debug/nurapods-daemon"
TRAY="$PROJECT_DIR/target/debug/nurapods-tray"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/nurapods"
KEYSTORE="$DATA_DIR/keystore"
STATE_DIR="${XDG_STATE_HOME:-$HOME/.local/state}/nurapods"
DAEMON_LOG="$STATE_DIR/daemon.log"
RUNTIME_DIR="${XDG_RUNTIME_DIR:-/tmp}/nurapods-${UID}"
DAEMON_PID_FILE="$RUNTIME_DIR/daemon.pid"
TRAY_PID_FILE="$RUNTIME_DIR/tray.pid"
DAEMON_PID=""
TRAY_PID=""

stop_pid_file() {
    local pid_file="$1" expected="$2" pid actual
    [[ -f "$pid_file" ]] || return 0
    read -r pid < "$pid_file" || true
    if [[ "${pid:-}" =~ ^[0-9]+$ ]] && kill -0 "$pid" 2>/dev/null; then
        actual="$(ps -p "$pid" -o comm= 2>/dev/null | tr -d '[:space:]')"
        if [[ "$actual" == "$expected" ]]; then
            kill "$pid" 2>/dev/null || true
        fi
    fi
    rm -f "$pid_file"
}

cleanup() {
    if [[ -n "$TRAY_PID" ]] && kill -0 "$TRAY_PID" 2>/dev/null; then
        kill "$TRAY_PID" 2>/dev/null || true
    fi
    if [[ -n "$DAEMON_PID" ]] && kill -0 "$DAEMON_PID" 2>/dev/null; then
        kill "$DAEMON_PID" 2>/dev/null || true
    fi
    [[ -n "$TRAY_PID" ]] && rm -f "$TRAY_PID_FILE"
    [[ -n "$DAEMON_PID" ]] && rm -f "$DAEMON_PID_FILE"
    [[ -n "$TRAY_PID" ]] && wait "$TRAY_PID" 2>/dev/null || true
    [[ -n "$DAEMON_PID" ]] && wait "$DAEMON_PID" 2>/dev/null || true
}

mkdir -p "$DATA_DIR" "$STATE_DIR"

case "${1:-start}" in
    start) ;;
    stop)
        stop_pid_file "$TRAY_PID_FILE" nurapods-tray
        stop_pid_file "$DAEMON_PID_FILE" nurapods-daemon
        echo "NuraPods chiuso (i servizi già attivi fuori da run.sh non vengono fermati)."
        exit 0
        ;;
    *)
        echo "Uso: $0 [start|stop]" >&2
        exit 2
        ;;
esac
trap cleanup EXIT INT TERM

echo "Controllo/buildo NuraPods..."
cargo build --workspace --manifest-path "$PROJECT_DIR/Cargo.toml"

# Reuse an already-running daemon when its local status socket responds.
if ! "$DAEMON" status --json >/dev/null 2>&1; then
    echo "Avvio il demone NuraPods..."
    mkdir -p "$RUNTIME_DIR"
    "$DAEMON" daemon "$KEYSTORE" >>"$DAEMON_LOG" 2>&1 &
    DAEMON_PID=$!
    printf '%s\n' "$DAEMON_PID" > "$DAEMON_PID_FILE"

    ready=false
    for _ in {1..20}; do
        if ! kill -0 "$DAEMON_PID" 2>/dev/null; then
            echo "Il demone non è partito. Log: $DAEMON_LOG" >&2
            tail -n 30 "$DAEMON_LOG" >&2 || true
            exit 1
        fi
        if "$DAEMON" status --json >/dev/null 2>&1; then
            ready=true
            break
        fi
        sleep 0.5
    done
    if [[ "$ready" != true ]]; then
        echo "Il demone non risponde ancora. Log: $DAEMON_LOG" >&2
        tail -n 30 "$DAEMON_LOG" >&2 || true
        exit 1
    fi
else
    echo "Demone NuraPods già attivo."
fi

echo "Avvio la tray NuraPods. Premi Ctrl+C per chiuderla."
mkdir -p "$RUNTIME_DIR"
trap cleanup EXIT INT TERM
"$TRAY" &
TRAY_PID=$!
printf '%s\n' "$TRAY_PID" > "$TRAY_PID_FILE"
wait "$TRAY_PID"
