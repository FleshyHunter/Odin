#!/usr/bin/env bash
# Runs both dev servers together from repo root: `./dev.sh`.
# set -m gives each backgrounded job its own process group, so on
# exit/Ctrl+C we can kill the whole group (cargo's compiled binary,
# vite's node process) instead of just the immediate subshell — a
# plain `kill 0` left those grandchildren running as orphans.
set -m
trap 'kill -- -$BACKEND_PID -$FRONTEND_PID 2>/dev/null' EXIT INT TERM

(cd backend && cargo run) &
BACKEND_PID=$!

(cd frontend && npm run dev) &
FRONTEND_PID=$!

wait
