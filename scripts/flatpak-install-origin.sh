#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# The development Flatpak's install origin (`make flatpak-install`).
#
#   scripts/flatpak-install-origin.sh path
#   scripts/flatpak-install-origin.sh install <repo> <ref> -- <export command...>
#   scripts/flatpak-install-origin.sh prepare <repo>
#   scripts/flatpak-install-origin.sh check
#   scripts/flatpak-install-origin.sh check --installed <repo> <ref>
#
# `flatpak install <path> <ref>` (what flatpak-builder --install runs) names the
# installed ref's origin remote `<last id component>-origin`, here
# `lushtext-origin`, with the path's file:// URI as its URL. When that path was
# a checkout's or a worktree's build cache, deleting the checkout leaves an
# enabled remote with no repository behind it, and GNOME Software and
# `flatpak update` then report "server has no summary file" on every refresh.
# So every checkout exports to ONE shared repository outside every checkout and
# cache (`path`), and `install` keeps `lushtext-origin` pointed at it:
#
#   1. run the export command (flatpak-builder --repo=<repo>, without
#      --install); if it fails, stop BEFORE touching any remote, so an enabled
#      origin never points at a repository that lacks the ref;
#   2. `prepare`: repoint and re-enable an existing lushtext-origin, because
#      flatpak reuses a disabled origin whose URL matches and then skips the
#      update with exit status 0;
#   3. `flatpak install --user -y --noninteractive --reinstall <repo> <ref>`,
#      the command flatpak-builder's --install runs;
#   4. prune the repository to its heads and run `check --installed`.
#
# All four steps run under flock on `<repo>.lock`, so two checkouts installing
# at once take turns. The lock is held by flock itself (`--close`): the export
# command does not inherit it, so a process a build leaves running cannot keep
# holding it.
#
# `check` fails when any user `<prefix>[0-9]*-origin` remote, enabled or
# disabled, has a file:// URL with no OSTree repository (config and summary)
# behind it, and prints the one-line fix; `make verify-flatpak-identity` runs
# it. `check --installed` is the post-condition of `install`.
#
# LUSHTEXT_FLATPAK_APP_ID (default dev.cominotti.lushtext) and
# LUSHTEXT_FLATPAK_INSTALL_REPO (default
# ${XDG_DATA_HOME:-$HOME/.local/share}/lushtext-flatpak-install/repo) exist for
# scripts/test-flatpak-install-origin.sh; a per-checkout repository would bring
# the dangling origin back.

set -euo pipefail

self="$(realpath -- "${BASH_SOURCE[0]}")"
app_id="${LUSHTEXT_FLATPAK_APP_ID:-dev.cominotti.lushtext}"
# flatpak's create_origin_remote_config: the last component of the id.
prefix="${app_id##*.}"
origin="$prefix-origin"
log_tag="flatpak-install-origin"

usage() {
    sed -n '6,10p' "$self" | sed 's/^# \{0,1\}//' >&2
    exit 2
}

die() {
    echo "$log_tag: error: $*" >&2
    exit 1
}

# The shared repository, canonical: flatpak compares origin URLs textually, and
# this host reaches the home directory as both /home and /var/home.
repo_path() {
    local data_home="${XDG_DATA_HOME:-$HOME/.local/share}"
    realpath -m -- "${LUSHTEXT_FLATPAK_INSTALL_REPO:-$data_home/lushtext-flatpak-install/repo}"
}

# The repository path must be absolute, canonical and made only of characters
# g_filename_to_uri never escapes, so `file://<path>` is byte-equal to the URI
# flatpak computes for the same path.
validate_repo() {
    local repo="$1"

    [[ "$repo" == /* ]] || die "the install repository '$repo' is not an absolute path."
    [[ "$repo" == "$(realpath -m -- "$repo")" ]] ||
        die "the install repository '$repo' is not canonical; pass the output of '$self path'."
    [[ "$repo" =~ ^[A-Za-z0-9._~/-]+$ ]] ||
        die "the install repository '$repo' contains characters outside [A-Za-z0-9._~/-], so its file:// URI would not match flatpak's; set LUSHTEXT_FLATPAK_INSTALL_REPO to a path without them."
}

# name<TAB>url<TAB>options for every user remote, disabled ones included.
user_remotes() {
    flatpak remotes --user --show-disabled --columns=name,url,options
}

remote_line() {
    user_remotes | awk -F'\t' -v n="$1" '$1 == n { print; exit }'
}

is_disabled() {
    [[ ",$1," == *,disabled,* ]]
}

state_word() {
    if is_disabled "$1"; then echo disabled; else echo enabled; fi
}

# The file:// URL's path, percent-decoded.
url_path() {
    local path="${1#file://}"
    printf '%b' "${path//%/\\x}"
}

# The commit at the head of <ref> in <repo>, read through flatpak (the install
# path needs no ostree CLI); empty when the repository lacks the ref.
repo_head() {
    local repo="$1" ref="$2" typed

    typed="$(flatpak repo --branches "$repo" 2>/dev/null |
        awk -F'\t' -v r="$ref" '$1 == "app/" r || $1 == "runtime/" r { print $1; exit }')" || true
    [[ -n "$typed" ]] || return 0
    flatpak repo --commits="$typed" "$repo" 2>/dev/null | awk '$1 == "Commit:" { print $2; exit }' || true
}

cmd_path() {
    repo_path
}

cmd_prepare() {
    local repo="$1" uri line url options

    validate_repo "$repo"
    uri="file://$repo"
    line="$(remote_line "$origin")"
    if [[ -z "$line" ]]; then
        echo "$log_tag: no $origin remote yet; the install creates it at $uri."
        return 0
    fi
    url="$(cut -f2 <<< "$line")"
    options="$(cut -f3 <<< "$line")"
    if [[ "$url" == "$uri" ]] && ! is_disabled "$options"; then
        echo "$log_tag: $origin already points at $uri."
        return 0
    fi
    flatpak remote-modify --user --enable --url="$uri" "$origin"
    echo "$log_tag: repointed $origin from $url ($(state_word "$options")) to $uri (enabled)."
}

cmd_check() {
    local shared shared_uri name url options path state errors=0 found=0

    shared="$(repo_path)"
    shared_uri="file://$shared"
    while IFS=$'\t' read -r name url options _; do
        [[ "$name" =~ ^${prefix}[0-9]*-origin$ ]] || continue
        [[ "$url" == file://* ]] || continue
        found=1
        path="$(url_path "$url")"
        state="$(state_word "$options")"
        if [[ ! -f "$path/config" || ! -f "$path/summary" ]]; then
            errors=$((errors + 1))
            {
                echo "error: the Flatpak remote '$name' ($state) points at $url, which is not an OSTree repository with a summary (no config or summary file there)."
                if [[ "$state" == enabled ]]; then
                    echo "  GNOME Software and 'flatpak update' report \"server has no summary file\" for it on every refresh."
                else
                    echo "  It is disabled, so refreshes stay quiet, but the development build installed from it can no longer be updated."
                fi
                echo "  Fix: run 'make flatpak-install' from any LushText checkout (it points $origin at $shared_uri),"
                echo "  or 'flatpak uninstall --user $app_id//master' to drop the development build; flatpak then removes the remote."
                if [[ "$name" != "$origin" ]]; then
                    echo "  If nothing is installed from '$name', 'flatpak remote-delete --user $name' removes it."
                fi
            } >&2
        elif [[ "$path" != "$shared" ]]; then
            echo "warning: the Flatpak remote '$name' ($state) points at $url, not the shared install repository $shared_uri; it breaks when that directory is deleted. 'make flatpak-install' repoints it."
        else
            echo "Install origin: $name ($state) at $url"
        fi
    done < <(user_remotes)
    if [[ "$errors" -gt 0 ]]; then
        return 1
    fi
    if [[ "$found" == 0 ]]; then
        echo "Install origin: no local $prefix*-origin remote."
    fi
}

cmd_check_installed() {
    local repo="$1" ref="$2" uri failures=() actual_origin line url options deployed head served

    validate_repo "$repo"
    uri="file://$repo"
    if ! actual_origin="$(flatpak info --user -o "$ref" 2>/dev/null)"; then
        die "$ref is not installed in the user installation."
    fi
    [[ "$actual_origin" == "$origin" ]] ||
        failures+=("$ref was installed from $actual_origin, expected $origin.")
    line="$(remote_line "$origin")"
    if [[ -z "$line" ]]; then
        failures+=("there is no user remote $origin.")
    else
        url="$(cut -f2 <<< "$line")"
        options="$(cut -f3 <<< "$line")"
        [[ "$url" == "$uri" ]] || failures+=("$origin points at $url, expected $uri.")
        if is_disabled "$options"; then
            failures+=("$origin is disabled, so flatpak skips its updates.")
        fi
    fi
    deployed="$(flatpak info --user -c "$ref")"
    head="$(repo_head "$repo" "$ref")"
    [[ "$deployed" == "$head" ]] ||
        failures+=("the deployed commit ${deployed:-<none>} is not the head ${head:-<none>} of $ref in $repo.")
    served="$(flatpak remote-info --user -c "$origin" "$ref" 2>/dev/null)" || served=""
    [[ "$served" == "$deployed" ]] ||
        failures+=("$origin serves ${served:-<nothing>} for $ref, but ${deployed:-<none>} is deployed.")
    if [[ "${#failures[@]}" -gt 0 ]]; then
        echo "$log_tag: error: the install did not deploy $ref from $origin at $uri:" >&2
        printf '  %s\n' "${failures[@]}" >&2
        exit 1
    fi
    echo "$log_tag: $ref deployed from $origin ($uri) at commit $deployed."
}

cmd_install() {
    local repo="$1" ref="$2" lock

    shift 2
    [[ "${1:-}" == "--" ]] || usage
    shift
    [[ "$#" -gt 0 ]] || usage
    [[ -n "$ref" ]] || usage
    validate_repo "$repo"
    command -v flock >/dev/null 2>&1 || die "flock (util-linux) is required to serialize installs."
    # build-export creates the repository but not its parent, which also holds the lock.
    mkdir -p -- "$(dirname -- "$repo")"
    lock="$repo.lock"
    if ! flock --nonblock --close "$lock" true; then
        echo "$log_tag: another install holds $lock; waiting for it to finish."
    fi
    exec flock --close "$lock" "$self" __locked "$repo" "$ref" -- "$@"
}

# Internal: the install sequence, run by flock while it holds the lock.
cmd_locked() {
    local repo="$1" ref="$2" status=0

    shift 3
    echo "$log_tag: exporting to $repo"
    "$@" || status=$?
    if [[ "$status" != 0 ]]; then
        echo "$log_tag: error: the export failed (exit $status); $origin was left unchanged." >&2
        exit "$status"
    fi
    cmd_prepare "$repo"
    flatpak install --user -y --noninteractive --reinstall "$repo" "$ref"
    flatpak build-update-repo --prune --prune-depth=0 "$repo"
    cmd_check_installed "$repo" "$ref"
}

case "${1:-}" in
    path)
        [[ "$#" == 1 ]] || usage
        cmd_path
        ;;
    install)
        [[ "$#" -ge 5 ]] || usage
        shift
        cmd_install "$@"
        ;;
    __locked)
        [[ "$#" -ge 5 ]] || usage
        shift
        cmd_locked "$@"
        ;;
    prepare)
        [[ "$#" == 2 ]] || usage
        cmd_prepare "$2"
        ;;
    check)
        if [[ "$#" == 1 ]]; then
            cmd_check
        elif [[ "$#" == 4 && "$2" == "--installed" ]]; then
            cmd_check_installed "$3" "$4"
        else
            usage
        fi
        ;;
    *)
        usage
        ;;
esac
