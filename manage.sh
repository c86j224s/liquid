#!/bin/bash

# .env 파일 로드
if [ -f .env ]; then
    set -a
    # shellcheck disable=SC1091
    . ./.env
    set +a
fi

APP_NAME="liquid"
LOG_FILE="server.log"
PID_FILE=".server.pid"

timestamp() {
    date -u +"%Y-%m-%dT%H:%M:%SZ"
}

append_log() {
    echo "[$(timestamp)] manage.sh $1" >> "$LOG_FILE"
}

is_valid_pid() {
    [[ "$1" =~ ^[1-9][0-9]*$ ]]
}

read_pid_file() {
    if [ ! -f "$PID_FILE" ]; then
        return 1
    fi

    PID=$(cat "$PID_FILE")
    if is_valid_pid "$PID"; then
        return 0
    fi

    append_log "removing invalid PID file value"
    rm -f "$PID_FILE"
    PID=""
    return 1
}

start() {
    if read_pid_file; then
        if ps -p "$PID" > /dev/null; then
            echo "⚠️  $APP_NAME is already running (PID: $PID)"
            return
        fi
        append_log "removing stale PID file for PID $PID"
        rm "$PID_FILE"
    fi

    echo "🚀 Starting $APP_NAME..."
    echo "   Host: $LIQUID_HOST"
    echo "   Port: $LIQUID_PORT"
    echo "   Data: $LIQUID_DATA_DIR"
    echo "   AI Workers Requested: ${LIQUID_AI_WORKERS:-1}"

    append_log "building debug binary"
    if ! cargo build --quiet >> "$LOG_FILE" 2>&1; then
        echo "❌ Failed to build $APP_NAME. Check $LOG_FILE for details."
        append_log "cargo build failed"
        return 1
    fi

    append_log "starting target/debug/$APP_NAME"
    RUST_BACKTRACE="${RUST_BACKTRACE:-1}" nohup "target/debug/$APP_NAME" >> "$LOG_FILE" 2>&1 &

    NEW_PID=$!
    echo "$NEW_PID" > "$PID_FILE"
    echo "✅ $APP_NAME started with PID: $NEW_PID"
    echo "   Logs are being written to $LOG_FILE"
}

stop() {
    if read_pid_file; then
        echo "🛑 Stopping $APP_NAME (PID: $PID)..."
        append_log "stop requested for PID $PID"
        kill -- "$PID"

        # 프로세스가 종료될 때까지 잠시 대기
        for i in {1..5}; do
            if ! ps -p "$PID" > /dev/null; then
                rm -f "$PID_FILE"
                append_log "server stopped after stop request"
                echo "✅ Stopped."
                return
            fi
            sleep 1
        done

        echo "⚠️  Force killing..."
        append_log "force killing PID $PID"
        kill -9 -- "$PID"
        rm -f "$PID_FILE"
        echo "✅ Force stopped."
    else
        # PID 파일이 없어도 이름으로 찾아서 종료
        PID=$(pgrep -f "target/debug/$APP_NAME")
        if [ -n "$PID" ]; then
            echo "🛑 Stopping $APP_NAME (PID: $PID) found by name..."
            kill -- $PID
            echo "✅ Stopped."
        else
            echo "❌ $APP_NAME is not running."
        fi
    fi
}

status() {
    if read_pid_file; then
        if ps -p "$PID" > /dev/null; then
            echo "🟢 $APP_NAME is running (PID: $PID)"
            echo "   URL: http://$LIQUID_HOST:$LIQUID_PORT"
            echo "   Recent Logs:"
            tail -n 5 "$LOG_FILE"
        else
            echo "🔴 $APP_NAME is not running (stale PID file found)"
        fi
    else
        echo "🔴 $APP_NAME is not running"
    fi
}

logs() {
    echo "📋 Showing last 20 lines of $LOG_FILE (use Ctrl+C to exit):"
    tail -f "$LOG_FILE"
}

case "$1" in
    start)
        start
        ;;
    stop)
        stop
        ;;
    status)
        status
        ;;
    logs)
        logs
        ;;
    restart)
        stop
        sleep 2
        start
        ;;
    *)
        echo "Usage: $0 {start|stop|status|logs|restart}"
        exit 1
esac
