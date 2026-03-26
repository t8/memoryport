#!/usr/bin/env bash
set -e

CONFIG="${UC_CONFIG:-$HOME/.memoryport/uc.toml}"
SERVER_PORT=8090
PROXY_PORT=9191
UI_PORT=5174

RED='\033[0;31m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
DIM='\033[2m'
NC='\033[0m'

kill_port() {
  local port=$1
  local pids=$(lsof -i :"$port" -t 2>/dev/null)
  if [ -n "$pids" ]; then
    echo "$pids" | xargs kill -9 2>/dev/null || true
    sleep 0.5
  fi
}

status() {
  echo ""
  for pair in "uc-server:$SERVER_PORT" "uc-proxy:$PROXY_PORT" "ui:$UI_PORT"; do
    name="${pair%%:*}"
    port="${pair##*:}"
    if curl -s -o /dev/null -w "" --connect-timeout 1 "http://localhost:$port/health" 2>/dev/null; then
      echo -e "  ${GREEN}●${NC} $name ${DIM}:$port${NC}"
    else
      echo -e "  ${RED}●${NC} $name ${DIM}:$port${NC}"
    fi
  done
  echo ""
}

build() {
  echo -e "${CYAN}Building...${NC}"
  cargo build -p uc-server -p uc-proxy 2>&1 | grep -E "Compiling|Finished|error" || true
}

start_server() {
  kill_port $SERVER_PORT
  echo -e "  Starting uc-server on :$SERVER_PORT"
  UC_SERVER_LISTEN="127.0.0.1:$SERVER_PORT" \
    nohup ./target/debug/uc-server --config "$CONFIG" \
    > /tmp/memoryport-server.log 2>&1 &
  echo $! > /tmp/memoryport-server.pid
}

start_proxy() {
  kill_port $PROXY_PORT
  echo -e "  Starting uc-proxy on :$PROXY_PORT"
  nohup ./target/debug/uc-proxy --config "$CONFIG" --listen "127.0.0.1:$PROXY_PORT" \
    > /tmp/memoryport-proxy.log 2>&1 &
  echo $! > /tmp/memoryport-proxy.pid
}

start_ui() {
  kill_port $UI_PORT
  echo -e "  Starting UI dev server on :$UI_PORT"
  cd ui && nohup pnpm dev > /tmp/memoryport-ui.log 2>&1 &
  echo $! > /tmp/memoryport-ui.pid
  cd ..
}

stop_all() {
  echo -e "${CYAN}Stopping all services...${NC}"
  kill_port $SERVER_PORT
  kill_port $PROXY_PORT
  kill_port $UI_PORT
  echo -e "  ${GREEN}All stopped${NC}"
}

start_all() {
  echo -e "${CYAN}Starting all services...${NC}"
  start_server
  sleep 1
  start_proxy
  start_ui
  sleep 3
  status
}

restart_all() {
  stop_all
  sleep 1
  build
  start_all
}

logs() {
  local service="${1:-server}"
  case "$service" in
    server) tail -f /tmp/memoryport-server.log ;;
    proxy)  tail -f /tmp/memoryport-proxy.log ;;
    ui)     tail -f /tmp/memoryport-ui.log ;;
    *)      echo "Usage: dev.sh logs [server|proxy|ui]" ;;
  esac
}

case "${1:-}" in
  start)    build && start_all ;;
  stop)     stop_all ;;
  restart)  restart_all ;;
  status)   status ;;
  build)    build ;;
  logs)     logs "$2" ;;
  server)   build && kill_port $SERVER_PORT && start_server && sleep 2 && status ;;
  proxy)    build && kill_port $PROXY_PORT && start_proxy && sleep 2 && status ;;
  ui)       kill_port $UI_PORT && start_ui && sleep 3 && status ;;
  *)
    echo "Usage: ./dev.sh {start|stop|restart|status|build|logs|server|proxy|ui}"
    echo ""
    echo "  start    - Build and start all services"
    echo "  stop     - Stop all services"
    echo "  restart  - Stop, rebuild, and start all"
    echo "  status   - Show which services are running"
    echo "  build    - Build Rust binaries only"
    echo "  logs X   - Tail logs (server|proxy|ui)"
    echo "  server   - Rebuild and restart just the server"
    echo "  proxy    - Rebuild and restart just the proxy"
    echo "  ui       - Restart just the UI dev server"
    ;;
esac
