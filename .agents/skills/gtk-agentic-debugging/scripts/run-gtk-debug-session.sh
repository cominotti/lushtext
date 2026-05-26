#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: run-gtk-debug-session.sh [options]

Record a live GTK debugging session into an artifact directory.

Options:
  --cmd COMMAND           Launcher command to run in a PTY (default: make run)
  --out DIR               Artifact directory (default: /tmp/gtk-debug-<timestamp>)
  --pid-pattern PATTERN   pgrep -f regex for the target app; prefer an anchored executable match
  --duration SECONDS      Keep monitors alive for N seconds after launcher exit
  --dbus-profile PROFILE  one of: shell, all, none (default: shell)
  --no-journal            Disable journalctl capture
  --help                  Show this help
EOF
}

command_to_run="make run"
out_dir=""
pid_pattern=""
duration=0
dbus_profile="shell"
capture_journal=1
interrupted=0
helper_pid="$$"
helper_parent_pid="${PPID:-}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --cmd)
      command_to_run="${2:?missing value for --cmd}"
      shift 2
      ;;
    --out)
      out_dir="${2:?missing value for --out}"
      shift 2
      ;;
    --pid-pattern)
      pid_pattern="${2:?missing value for --pid-pattern}"
      shift 2
      ;;
    --duration)
      duration="${2:?missing value for --duration}"
      shift 2
      ;;
    --dbus-profile)
      dbus_profile="${2:?missing value for --dbus-profile}"
      shift 2
      ;;
    --no-journal)
      capture_journal=0
      shift
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Missing required command: $1" >&2
    exit 1
  fi
}

require_cmd script
require_cmd stdbuf
require_cmd ps
require_cmd sed
require_cmd awk
require_cmd python3

timestamp="$(date +%Y%m%d-%H%M%S)"
if [[ -z "$out_dir" ]]; then
  out_dir="/tmp/gtk-debug-${timestamp}"
fi
mkdir -p "$out_dir"

app_log="$out_dir/app.typescript"
dbus_log="$out_dir/dbus.log"
journal_log="$out_dir/journal.log"
status_log="$out_dir/status.txt"
session_env="$out_dir/session.env"
process_before="$out_dir/process-before.txt"
process_after="$out_dir/process-after.txt"

cleanup_pids=()
cleanup() {
  local pid
  for pid in "${cleanup_pids[@]:-}"; do
    if kill -0 "$pid" >/dev/null 2>&1; then
      kill "$pid" >/dev/null 2>&1 || true
      wait "$pid" 2>/dev/null || true
    fi
  done
}
on_signal() {
  interrupted=1
}
trap cleanup EXIT
trap on_signal INT TERM

collect_pids() {
  local pattern="${1:-}"
  if [[ -z "$pattern" ]]; then
    echo ""
    return 0
  fi
  (
    pgrep_lines "$pattern" | awk '{ print $1 }'
  ) | sort -n | tr '\n' ' ' | sed 's/[[:space:]]*$//'
}

pgrep_lines() {
  local pattern="${1:-}"
  if [[ -z "$pattern" ]]; then
    return 0
  fi
  (
    pgrep -af -- "$pattern" 2>/dev/null || true
  ) | awk -v self="$helper_pid" -v parent="$helper_parent_pid" '
    $1 == self || $1 == parent { next }
    index($0, "run-gtk-debug-session.sh") > 0 { next }
    index($0, "pgrep -") > 0 { next }
    { print }
  '
}

warn_if_pid_pattern_is_contaminated() {
  local pattern="${1:-}"
  if [[ -z "$pattern" ]]; then
    return 0
  fi

  local raw_matches
  raw_matches="$(pgrep -af -- "$pattern" 2>/dev/null || true)"
  if [[ -z "$raw_matches" ]]; then
    return 0
  fi

  if printf '%s\n' "$raw_matches" | awk '
    index($0, "run-gtk-debug-session.sh") > 0 || index($0, "pgrep -") > 0 { found = 1 }
    END { exit(found ? 0 : 1) }
  '; then
    local warning="pid-pattern matched helper or probe processes; tighten it (for example '(^| )target/debug/lushtext($| )')"
    echo "pid_pattern_warning=$warning" >>"$session_env"
    echo "WARNING: $warning" | tee -a "$status_log"
  fi
}

{
  echo "timestamp=$timestamp"
  echo "cwd=$(pwd)"
  echo "command=$command_to_run"
  echo "pid_pattern=$pid_pattern"
  echo "helper_pid=$helper_pid"
  echo "helper_parent_pid=$helper_parent_pid"
  echo "dbus_profile=$dbus_profile"
  echo "DISPLAY=${DISPLAY:-}"
  echo "WAYLAND_DISPLAY=${WAYLAND_DISPLAY:-}"
  echo "XDG_SESSION_TYPE=${XDG_SESSION_TYPE:-}"
  echo "DBUS_SESSION_BUS_ADDRESS=${DBUS_SESSION_BUS_ADDRESS:-}"
  echo "script=$(command -v script)"
  echo "stdbuf=$(command -v stdbuf)"
  command -v dbus-monitor >/dev/null 2>&1 && echo "dbus-monitor=$(command -v dbus-monitor)"
  command -v journalctl >/dev/null 2>&1 && echo "journalctl=$(command -v journalctl)"
} >"$session_env"

if [[ -n "$pid_pattern" ]]; then
  warn_if_pid_pattern_is_contaminated "$pid_pattern"
  pgrep_lines "$pid_pattern" >"$process_before" || true
else
  : >"$process_before"
fi

echo "Artifacts: $out_dir" | tee -a "$status_log"

if command -v dbus-monitor >/dev/null 2>&1 && [[ "$dbus_profile" != "none" ]]; then
  case "$dbus_profile" in
    shell)
      dbus-monitor --session \
        "type='signal',sender='org.gnome.Shell'" \
        "type='signal',sender='org.freedesktop.portal.Desktop'" \
        >"$dbus_log" 2>&1 &
      ;;
    all)
      dbus-monitor --session >"$dbus_log" 2>&1 &
      ;;
    *)
      echo "Unknown dbus profile: $dbus_profile" >&2
      exit 1
      ;;
  esac
  cleanup_pids+=("$!")
  echo "dbus_monitor_pid=${cleanup_pids[-1]}" >>"$session_env"
fi

if (( capture_journal )) && command -v journalctl >/dev/null 2>&1; then
  journalctl --user -f -o short-precise --since "now" >"$journal_log" 2>&1 &
  cleanup_pids+=("$!")
  echo "journal_pid=${cleanup_pids[-1]}" >>"$session_env"
fi

before_pids="$(collect_pids "$pid_pattern")"
echo "before_pids=${before_pids}" >>"$session_env"

echo "Running launcher in PTY: $command_to_run" | tee -a "$status_log"
set +e
script -qefc "stdbuf -oL -eL bash -lc $(printf '%q' "$command_to_run")" "$app_log"
launcher_status=$?
set -e
echo "launcher_status=$launcher_status" | tee -a "$status_log"

after_pids="$(collect_pids "$pid_pattern")"
echo "after_pids=${after_pids}" >>"$session_env"

if [[ -n "$pid_pattern" ]]; then
  pgrep_lines "$pid_pattern" >"$process_after" || true
else
  : >"$process_after"
fi

if [[ -n "$pid_pattern" ]] && [[ -n "$after_pids" ]]; then
  if [[ "$before_pids" == "$after_pids" ]]; then
    echo "launcher_note=launcher returned without a new matching pid; inspect launcher output for a handoff, failed relaunch, or stale pid pattern" \
      | tee -a "$status_log"
  else
    echo "launcher_note=matching pid set changed after launch" | tee -a "$status_log"
  fi
fi

if [[ -n "$pid_pattern" ]] && [[ -n "$after_pids" ]] && (( duration > 0 )); then
  echo "Keeping monitors alive for ${duration}s after launcher exit" | tee -a "$status_log"
  sleep "$duration" || true
elif [[ -n "$pid_pattern" ]] && [[ -n "$after_pids" ]]; then
  echo "Monitoring continues while matching pids exist; press Ctrl-C when the repro is finished" \
    | tee -a "$status_log"
  while [[ -n "$(collect_pids "$pid_pattern")" ]]; do
    if (( interrupted )); then
      break
    fi
    sleep 1 || true
  done
fi

cleanup
cleanup_pids=()
python3 "$(dirname "$0")/summarize-runtime-logs.py" "$out_dir" >"$out_dir/summary.md" || true
echo "Summary: $out_dir/summary.md" | tee -a "$status_log"

exit "$launcher_status"
