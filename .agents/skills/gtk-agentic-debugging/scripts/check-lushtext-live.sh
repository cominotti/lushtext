#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: check-lushtext-live.sh [options]

Check whether LushText is currently alive before live GTK interaction.

Options:
  --dbus-name NAME        Application D-Bus name (default: dev.cominotti.lushtext)
  --pid-pattern PATTERN   Extra pgrep -f regex for the app process
  --session DIR           Debug artifact dir with session.env from run-gtk-debug-session.sh
  --require-dbus          Fail unless the application D-Bus name has an owner
  --require-launched-instance
                          Fail unless a current matching PID was not present before launch
  --require-tool COMMAND   Fail unless COMMAND is installed; repeat as needed
  --help                  Show this help
EOF
}

dbus_name="${LUSHTEXT_DEBUG_APP_ID:-dev.cominotti.lushtext}"
pid_pattern=""
session_dir=""
require_dbus=0
require_launched_instance=0
required_tools=()
helper_pid="$$"
helper_parent_pid="${PPID:-}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dbus-name)
      dbus_name="${2:?missing value for --dbus-name}"
      shift 2
      ;;
    --pid-pattern)
      pid_pattern="${2:?missing value for --pid-pattern}"
      shift 2
      ;;
    --session)
      session_dir="${2:?missing value for --session}"
      shift 2
      ;;
    --require-dbus)
      require_dbus=1
      shift
      ;;
    --require-launched-instance)
      require_launched_instance=1
      shift
      ;;
    --require-tool)
      required_tools+=("${2:?missing value for --require-tool}")
      shift 2
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

read_session_value() {
  local key="$1"
  local env_file="${session_dir}/session.env"

  if [[ -z "$session_dir" || ! -f "$env_file" ]]; then
    return 1
  fi

  sed -n "s/^${key}=//p" "$env_file" | tail -n 1
}

check_required_tools() {
  local missing=()
  local tool
  local ydotool_socket

  for tool in "${required_tools[@]}"; do
    if [[ "$tool" == "pyatspi" ]]; then
      if ! /usr/bin/python3 -c 'import gi, pyatspi' >/dev/null 2>&1; then
        missing+=("pyatspi")
      fi
      continue
    fi

    if ! command -v "$tool" >/dev/null 2>&1; then
      missing+=("$tool")
    fi
  done

  if (( ${#missing[@]} > 0 )); then
    echo "Missing required tool(s): ${missing[*]}" >&2
    echo "Install them before interacting with or snapshotting LushText." >&2
    echo "For this repo, try: make dev-tools" >&2
    exit 2
  fi

  for tool in "${required_tools[@]}"; do
    if [[ "$tool" == "pyatspi" ]]; then
      continue
    fi

    if [[ "$tool" == "ydotool" ]]; then
      ydotool_socket="${YDOTOOL_SOCKET:-${XDG_RUNTIME_DIR:-/tmp}/.ydotool_socket}"
      if [[ ! -S "$ydotool_socket" ]] ||
        ! YDOTOOL_SOCKET="$ydotool_socket" ydotool type "" >/dev/null 2>&1; then
        echo "ydotool is installed but its daemon is not usable at ${ydotool_socket}." >&2
        echo "Run make dev-tools before using ydotool-driven input." >&2
        exit 2
      fi
    fi
  done
}

name_has_owner() {
  if ! command -v gdbus >/dev/null 2>&1; then
    return 1
  fi

  local output
  output="$(
    gdbus call --session \
      --dest org.freedesktop.DBus \
      --object-path /org/freedesktop/DBus \
      --method org.freedesktop.DBus.NameHasOwner \
      "$dbus_name" 2>/dev/null || true
  )"

  [[ "$output" == "(true,)" ]]
}

pgrep_lines() {
  local pattern="$1"

  if [[ -z "$pattern" ]]; then
    return 0
  fi

  (
    pgrep -af -- "$pattern" 2>/dev/null || true
  ) | awk -v self="$helper_pid" -v parent="$helper_parent_pid" '
    $1 == self || $1 == parent { next }
    index($0, "check-lushtext-live.sh") > 0 { next }
    index($0, "pgrep -") > 0 { next }
    { print }
  '
}

collect_pids() {
  local pattern="$1"

  if [[ -z "$pattern" ]]; then
    echo ""
    return 0
  fi

  pgrep_lines "$pattern" | awk '{ print $1 }' | sort -n | tr '\n' ' ' | sed 's/[[:space:]]*$//'
}

pid_is_in_list() {
  local needle="$1"
  local haystack="$2"
  local pid

  for pid in $haystack; do
    if [[ "$pid" == "$needle" ]]; then
      return 0
    fi
  done

  return 1
}

new_pids_since_launch() {
  local current="$1"
  local before="$2"
  local pid
  local new_pids=()

  for pid in $current; do
    if ! pid_is_in_list "$pid" "$before"; then
      new_pids+=("$pid")
    fi
  done

  printf '%s ' "${new_pids[@]}" | sed 's/[[:space:]]*$//'
}

check_required_tools

if [[ -z "$pid_pattern" ]]; then
  pid_pattern="$(read_session_value pid_pattern || true)"
fi

if (( require_launched_instance )); then
  if [[ -z "$session_dir" ]]; then
    echo "--require-launched-instance needs --session so the pre-launch PID set can be checked." >&2
    exit 2
  fi

  if [[ -z "$pid_pattern" ]]; then
    echo "--require-launched-instance needs a pid pattern from --pid-pattern or session.env." >&2
    exit 2
  fi
fi

dbus_live=0
process_live=0
process_matches=""
current_pids=""
before_pids="$(read_session_value before_pids || true)"
launched_pids=""

if name_has_owner; then
  dbus_live=1
fi

if [[ -n "$pid_pattern" ]]; then
  process_matches="$(pgrep_lines "$pid_pattern")"
  current_pids="$(collect_pids "$pid_pattern")"
  launched_pids="$(new_pids_since_launch "$current_pids" "$before_pids")"
  if [[ -n "$process_matches" ]]; then
    process_live=1
  fi
fi

echo "dbus_name=${dbus_name}"
echo "dbus_live=${dbus_live}"
if [[ -n "$pid_pattern" ]]; then
  echo "pid_pattern=${pid_pattern}"
  echo "process_live=${process_live}"
  echo "current_pids=${current_pids}"
  if [[ -n "$session_dir" ]]; then
    echo "session=${session_dir}"
    echo "before_pids=${before_pids}"
    echo "launched_pids=${launched_pids}"
  fi
  if [[ -n "$process_matches" ]]; then
    echo "process_matches:"
    printf '%s\n' "$process_matches"
  fi
fi

if (( require_dbus )) && (( ! dbus_live )); then
  echo "LushText is not ready for D-Bus-driven interaction." >&2
  exit 1
fi

if (( require_launched_instance )) && [[ -z "$launched_pids" ]]; then
  echo "No current LushText PID is proven to belong to this debug launch." >&2
  echo "Refusing to interact with a possibly pre-existing instance." >&2
  exit 1
fi

if (( dbus_live || process_live )); then
  echo "LushText is running."
  exit 0
fi

echo "LushText is not running." >&2
exit 1
