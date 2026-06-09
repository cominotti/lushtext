#!/usr/bin/env bash
set -euo pipefail

packages=(
    ydotool
    gnome-screenshot
    at-spi2-core
    python3-pyatspi
    python3-gobject
    xorg-x11-server-Xvfb
    xwd
    ImageMagick
    xdotool
    gstreamer1
    pipewire
    pipewire-utils
    pipewire-gstreamer
    wireplumber
    gstreamer1-plugins-base
    gstreamer1-plugins-good
    mutter
    dbus-daemon
    glib2
    blueprint-compiler
)
runtime_dir="${XDG_RUNTIME_DIR:-/tmp}"
socket_path="${YDOTOOL_SOCKET:-${runtime_dir}/.ydotool_socket}"
log_path="${runtime_dir}/lushtext-ydotoold.log"
dry_run="${LUSHTEXT_DEV_TOOLS_DRY_RUN:-0}"

log() {
    printf '%s\n' "$*"
}

run_cmd() {
    if [[ "$dry_run" == "1" ]]; then
        printf '+'
        printf ' %q' "$@"
        printf '\n'
        return 0
    fi

    "$@"
}

package_for_command() {
    case "$1" in
        dbus-run-session)
            printf 'dbus-daemon'
            ;;
        gdbus | gsettings)
            printf 'glib2'
            ;;
        ydotool | ydotoold)
            printf 'ydotool'
            ;;
        gnome-screenshot)
            printf 'gnome-screenshot'
            ;;
        gst-launch-1.0 | gst-inspect-1.0)
            printf 'gstreamer1'
            ;;
        magick)
            printf 'ImageMagick'
            ;;
        mutter)
            printf 'mutter'
            ;;
        pipewire)
            printf 'pipewire'
            ;;
        pw-dump)
            printf 'pipewire-utils'
            ;;
        wireplumber)
            printf 'wireplumber'
            ;;
        Xvfb)
            printf 'xorg-x11-server-Xvfb'
            ;;
        blueprint-compiler)
            printf 'blueprint-compiler'
            ;;
        xdotool)
            printf 'xdotool'
            ;;
        xwd)
            printf 'xwd'
            ;;
        *)
            return 1
            ;;
    esac
}

system_python() {
    if [[ -x /usr/bin/python3 ]]; then
        printf '/usr/bin/python3'
        return 0
    fi

    command -v python3
}

append_unique_package() {
    local package="$1"

    for existing in "${missing_packages[@]}"; do
        if [[ "$existing" == "$package" ]]; then
            return 0
        fi
    done

    missing_packages+=("$package")
}

install_missing_packages() {
    local required_commands=(
        dbus-run-session
        gdbus
        gnome-screenshot
        gst-inspect-1.0
        gst-launch-1.0
        gsettings
        magick
        mutter
        pipewire
        pw-dump
        wireplumber
        Xvfb
        blueprint-compiler
        xdotool
        xwd
        ydotool
        ydotoold
    )
    local python
    missing_packages=()

    for command_name in "${required_commands[@]}"; do
        if ! command -v "$command_name" >/dev/null 2>&1; then
            append_unique_package "$(package_for_command "$command_name")"
        fi
    done

    python="$(system_python)"
    if ! "$python" -c 'import gi, pyatspi' >/dev/null 2>&1; then
        append_unique_package "python3-pyatspi"
        append_unique_package "python3-gobject"
    fi

    if [[ ! -x /usr/libexec/at-spi2-registryd ]]; then
        append_unique_package "at-spi2-core"
    fi

    if command -v gst-inspect-1.0 >/dev/null 2>&1; then
        if ! gst-inspect-1.0 pipewiresrc >/dev/null 2>&1; then
            append_unique_package "pipewire-gstreamer"
        fi
        if ! gst-inspect-1.0 videoconvert >/dev/null 2>&1; then
            append_unique_package "gstreamer1-plugins-base"
        fi
        if ! gst-inspect-1.0 pngenc >/dev/null 2>&1; then
            append_unique_package "gstreamer1-plugins-good"
        fi
    else
        append_unique_package "gstreamer1"
        append_unique_package "pipewire-gstreamer"
        append_unique_package "gstreamer1-plugins-base"
        append_unique_package "gstreamer1-plugins-good"
    fi

    if (( ${#missing_packages[@]} == 0 )); then
        log "Development helper packages are already installed."
        return 0
    fi

    log "Installing missing development helper packages: ${missing_packages[*]}"

    if command -v dnf >/dev/null 2>&1; then
        run_cmd sudo dnf install -y "${missing_packages[@]}"
        return 0
    fi

    if command -v rpm-ostree >/dev/null 2>&1; then
        if [[ "${LUSHTEXT_DEV_TOOLS_ALLOW_RPM_OSTREE:-0}" == "1" ]]; then
            run_cmd rpm-ostree install "${missing_packages[@]}"
            log "rpm-ostree may require a reboot before the new commands are available."
            return 0
        fi

        log "rpm-ostree detected, but host layering is disabled by default."
        log "Run this target inside Toolbx/dnf, or set LUSHTEXT_DEV_TOOLS_ALLOW_RPM_OSTREE=1 if host layering is intentional."
        return 1
    fi

    log "Could not find dnf or rpm-ostree. Install these packages manually: ${packages[*]}"
    return 1
}

ydotoold_supports() {
    local flag="$1"

    ydotoold --help 2>&1 | grep -q -- "$flag"
}

ydotool_socket_healthy() {
    YDOTOOL_SOCKET="$socket_path" ydotool type "" >/dev/null 2>&1
}

setup_ydotool_daemon() {
    if [[ "$dry_run" == "1" ]]; then
        log "Dry run: would configure ydotoold on ${socket_path}."
        return 0
    fi

    if ! command -v ydotoold >/dev/null 2>&1 || ! command -v ydotool >/dev/null 2>&1; then
        log "ydotool is not available yet; rerun this target after package installation completes."
        return 1
    fi

    if [[ ! -e /dev/uinput ]]; then
        log "Skipping ydotoold setup: /dev/uinput is not present in this environment."
        log "On Fedora Toolbx, enter a toolbox with /dev/uinput exposed or start ydotoold on the host."
        return 0
    fi

    if [[ ! -w /dev/uinput ]]; then
        log "Skipping ydotoold setup: $(id -un) cannot write to /dev/uinput."
        log "Grant access on the host, then rerun make dev-tools."
        return 0
    fi

    if [[ -S "$socket_path" ]]; then
        if ydotool_socket_healthy; then
            log "ydotool socket is already usable at ${socket_path}; leaving it in place."
            return 0
        fi

        log "Removing stale ydotool socket at ${socket_path}."
        rm -f "$socket_path"
    fi

    local socket_dir
    socket_dir="$(dirname "$socket_path")"
    mkdir -p "$socket_dir"

    local args=(--socket-path "$socket_path")

    if ydotoold_supports '--socket-own'; then
        args+=(--socket-own "$(id -u):$(id -g)")
    fi

    if ydotoold_supports '--socket-perm'; then
        args+=(--socket-perm 0600)
    fi

    log "Starting ydotoold at ${socket_path}."
    nohup ydotoold "${args[@]}" >"$log_path" 2>&1 &

    for _ in {1..30}; do
        if [[ -S "$socket_path" ]] && ydotool_socket_healthy; then
            log "ydotoold is ready. ydotool will use ${socket_path}."
            return 0
        fi
        sleep 0.1
    done

    log "ydotoold did not create ${socket_path}. See ${log_path} for details."
    return 1
}

main() {
    install_missing_packages
    setup_ydotool_daemon
    log "Development helper tools are ready."
}

main "$@"
