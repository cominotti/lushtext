#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Release helper for LushText.
#
# Usage:
#   ./scripts/release.sh tag <version> [yes] [dry_run]
#   ./scripts/release.sh bump <type> [prerelease] [promote] [yes] [dry_run]
#   ./scripts/release.sh validate-tag <version> [skip_flatpak]

set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_root="${LUSHTEXT_RELEASE_REPO_ROOT:-$(cd -- "$script_dir/.." && pwd)}"
cd "$repo_root"

app_id="dev.cominotti.lushtext"
local_flatpak_manifest="build-aux/dev.cominotti.lushtext.Flatpak.json"
metainfo_file="data/dev.cominotti.lushtext.metainfo.xml.in"
desktop_file="data/dev.cominotti.lushtext.desktop.in"
cargo_sources_file="build-aux/cargo-sources.json"

release_files=(
    meson.build
    crates/lushtext/Cargo.toml
    crates/lushtext-core/Cargo.toml
    Cargo.lock
    "$metainfo_file"
    "$cargo_sources_file"
)

if [[ -t 1 ]]; then
    red=$'\033[0;31m'
    green=$'\033[0;32m'
    yellow=$'\033[0;33m'
    cyan=$'\033[0;36m'
    bold=$'\033[1m'
    reset=$'\033[0m'
else
    red=""
    green=""
    yellow=""
    cyan=""
    bold=""
    reset=""
fi

die() { printf '%sERROR:%s %s\n' "$red" "$reset" "$*" >&2; exit 1; }
warn() { printf '%sWARNING:%s %s\n' "$yellow" "$reset" "$*" >&2; }
info() { printf '%s%s%s\n' "$cyan" "$*" "$reset"; }
ok() { printf '%s%s%s\n' "$green" "$*" "$reset"; }

validate_semver() {
    local version="$1"

    if [[ ! "$version" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-[A-Za-z0-9]+(\.[A-Za-z0-9]+)*)?$ ]]; then
        die "invalid semver '$version' (expected v1.2.3 or v1.2.3-alpha.1)"
    fi
}

plain_version() {
    printf '%s\n' "${1#v}"
}

base_version() {
    printf '%s\n' "${1%%-*}"
}

major_part() {
    plain_version "$1" | cut -d. -f1
}

minor_part() {
    plain_version "$1" | cut -d. -f2
}

patch_part() {
    local patch
    patch="$(plain_version "$1" | cut -d. -f3)"
    printf '%s\n' "${patch%%-*}"
}

latest_stable_tag() {
    git tag --list 'v*' 2>/dev/null |
        { grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' || true; } |
        sort -V |
        tail -1
}

latest_prerelease_tag() {
    local base="$1"
    local label="$2"

    git tag --list "${base}-${label}.*" 2>/dev/null |
        sort -V |
        tail -1
}

all_prerelease_tags() {
    local base="$1"

    git tag --list "${base}-*" 2>/dev/null |
        { grep -E "^${base}-[A-Za-z0-9]+\\.[0-9]+$" || true; } |
        sort -V
}

bump_version() {
    local version="$1"
    local bump_type="$2"
    local major minor patch

    major="$(major_part "$version")"
    minor="$(minor_part "$version")"
    patch="$(patch_part "$version")"

    case "$bump_type" in
        major) printf 'v%d.0.0\n' "$((major + 1))" ;;
        minor) printf 'v%d.%d.0\n' "$major" "$((minor + 1))" ;;
        patch) printf 'v%d.%d.%d\n' "$major" "$minor" "$((patch + 1))" ;;
        *) die "invalid TYPE '$bump_type' (expected major, minor, or patch)" ;;
    esac
}

compute_next_version() {
    local bump_type="$1"
    local prerelease="${2:-}"
    local promote="${3:-}"
    local latest target latest_pre current_num existing_prereleases

    case "$bump_type" in
        major|minor|patch) ;;
        *) die "invalid TYPE '$bump_type' (expected major, minor, or patch)" ;;
    esac

    if [[ -n "$prerelease" ]]; then
        case "$prerelease" in
            alpha|beta|rc) ;;
            *) die "invalid PRERELEASE '$prerelease' (expected alpha, beta, or rc)" ;;
        esac
    fi

    latest="$(latest_stable_tag)"
    if [[ -z "$latest" ]]; then
        latest="v0.0.0"
        info "No stable tags found; starting from v0.0.0"
    else
        info "Latest stable tag: ${bold}${latest}${reset}"
    fi

    target="$(bump_version "$latest" "$bump_type")"

    if [[ -z "$prerelease" ]]; then
        existing_prereleases="$(all_prerelease_tags "$target")"
        if [[ -n "$existing_prereleases" && "$promote" != "1" ]]; then
            printf '\n'
            warn "Prerelease tags exist for $target:"
            printf '%s\n' "$existing_prereleases" | sed 's/^/  - /'
            die "pass PROMOTE=1 to promote this prerelease stream to stable"
        fi
        if [[ -n "$existing_prereleases" ]]; then
            info "Promoting prerelease stream for $target"
        fi
        next_version="$target"
        return 0
    fi

    latest_pre="$(latest_prerelease_tag "$target" "$prerelease")"
    if [[ -n "$latest_pre" ]]; then
        current_num="$(grep -oE '[0-9]+$' <<< "$latest_pre")"
        next_version="${target}-${prerelease}.$((current_num + 1))"
        info "Continuing prerelease stream: ${latest_pre} -> ${next_version}"
    else
        next_version="${target}-${prerelease}.1"
        info "Starting prerelease stream: ${next_version}"
    fi
}

xml_escape() {
    sed \
        -e 's/&/\&amp;/g' \
        -e 's/</\&lt;/g' \
        -e 's/>/\&gt;/g'
}

release_notes_xml() {
    local notes_file="$1"
    local emitted=0 line escaped

    [[ -f "$notes_file" ]] || die "release notes file does not exist: $notes_file"

    while IFS= read -r line || [[ -n "$line" ]]; do
        if [[ -z "${line//[[:space:]]/}" ]]; then
            continue
        fi
        escaped="$(printf '%s' "$line" | xml_escape)"
        printf '        <p>%s</p>\n' "$escaped"
        emitted=1
    done < "$notes_file"

    [[ "$emitted" == "1" ]] || die "release notes file is empty: $notes_file"
}

set_first_toml_package_version() {
    local file="$1"
    local version="$2"
    local tmp

    tmp="$(mktemp)"
    awk -v version="$version" '
        !done && /^version = "/ {
            print "version = \"" version "\""
            done = 1
            next
        }
        { print }
    ' "$file" > "$tmp"
    mv "$tmp" "$file"
}

set_meson_version() {
    local version="$1"

    sed -i -E "0,/version: '[^']+'/s//version: '$version'/" meson.build
}

appstream_release_block() {
    local version="$1"
    local notes_file="$2"
    local date="${LUSHTEXT_RELEASE_DATE:-$(date -u +%F)}"
    local notes

    notes="$(release_notes_xml "$notes_file")"
    cat <<EOF
    <release version="$version" date="$date">
      <description>
$notes
      </description>
    </release>
EOF
}

insert_appstream_release() {
    local version="$1"
    local notes_file="$2"
    local block tmp

    block="$(appstream_release_block "$version" "$notes_file")"

    tmp="$(mktemp)"
    awk -v block="$block" '
        /^[[:space:]]*<releases>[[:space:]]*$/ && !inserted {
            print
            print block
            inserted = 1
            next
        }
        { print }
        END { if (!inserted) exit 7 }
    ' "$metainfo_file" > "$tmp" || {
        rm -f "$tmp"
        die "could not insert AppStream release into $metainfo_file"
    }
    mv "$tmp" "$metainfo_file"
}

replace_appstream_release() {
    local version="$1"
    local notes_file="$2"
    local block tmp

    block="$(appstream_release_block "$version" "$notes_file")"

    tmp="$(mktemp)"
    awk -v version="$version" -v block="$block" '
        $0 ~ "<release version=\"" version "\"" && !replaced {
            print block
            replacing = 1
            replaced = 1
            next
        }
        replacing && /^[[:space:]]*<\/release>[[:space:]]*$/ {
            replacing = 0
            next
        }
        replacing {
            next
        }
        { print }
        END { if (!replaced) exit 7 }
    ' "$metainfo_file" > "$tmp" || {
        rm -f "$tmp"
        die "could not replace AppStream release '$version' in $metainfo_file"
    }
    mv "$tmp" "$metainfo_file"
}

upsert_appstream_release() {
    local version="$1"
    local notes_file="$2"

    if grep -Fq "<release version=\"$version\"" "$metainfo_file"; then
        replace_appstream_release "$version" "$notes_file"
    else
        insert_appstream_release "$version" "$notes_file"
    fi
}

cargo_lock_version_for() {
    local package="$1"

    awk -v package="$package" '
        $0 == "[[package]]" {
            in_package = 1
            matched = 0
            next
        }
        in_package && $0 == "name = \"" package "\"" {
            matched = 1
            next
        }
        in_package && matched && /^version = "/ {
            value = $0
            sub(/^version = "/, "", value)
            sub(/"$/, "", value)
            print value
            exit
        }
    ' Cargo.lock
}

verify_version_surfaces() {
    local version="$1"
    local plain

    plain="$(plain_version "$version")"

    grep -Fq "version: '$plain'" meson.build ||
        die "meson.build does not contain release version '$plain'"
    grep -Fq "version = \"$plain\"" crates/lushtext/Cargo.toml ||
        die "crates/lushtext/Cargo.toml does not contain release version '$plain'"
    grep -Fq "version = \"$plain\"" crates/lushtext-core/Cargo.toml ||
        die "crates/lushtext-core/Cargo.toml does not contain release version '$plain'"
    [[ "$(cargo_lock_version_for lushtext)" == "$plain" ]] ||
        die "Cargo.lock does not contain lushtext version '$plain'"
    [[ "$(cargo_lock_version_for lushtext-core)" == "$plain" ]] ||
        die "Cargo.lock does not contain lushtext-core version '$plain'"
    grep -Fq "<release version=\"$plain\"" "$metainfo_file" ||
        die "$metainfo_file does not contain AppStream release '$plain'"
}

refresh_cargo_lock() {
    command -v cargo >/dev/null 2>&1 || die "cargo is required to refresh Cargo.lock"
    cargo metadata --format-version 1 --no-deps >/dev/null
}

verify_cargo_sources_current() {
    local tmp

    command -v flatpak-cargo-generator >/dev/null 2>&1 ||
        die "flatpak-cargo-generator is required to verify $cargo_sources_file"
    tmp="$(mktemp)"
    flatpak-cargo-generator Cargo.lock -o "$tmp" >/dev/null
    if ! cmp -s "$tmp" "$cargo_sources_file"; then
        rm -f "$tmp"
        die "$cargo_sources_file is stale; run make cargo-sources"
    fi
    rm -f "$tmp"
}

refresh_cargo_sources() {
    command -v flatpak-cargo-generator >/dev/null 2>&1 ||
        die "flatpak-cargo-generator is required to regenerate $cargo_sources_file"
    flatpak-cargo-generator Cargo.lock -o "$cargo_sources_file"
}

validate_appstream() {
    command -v appstreamcli >/dev/null 2>&1 ||
        die "appstreamcli is required for release metadata validation"
    appstreamcli validate --no-net --explain "$metainfo_file"

    if command -v flatpak-builder-lint >/dev/null 2>&1; then
        flatpak-builder-lint appstream "$metainfo_file"
    elif command -v flatpak >/dev/null 2>&1 && flatpak info org.flatpak.Builder >/dev/null 2>&1; then
        flatpak run --command=flatpak-builder-lint org.flatpak.Builder appstream "$metainfo_file"
    else
        warn "org.flatpak.Builder is not installed; AppStream CLI validation passed but Flathub-specific lint was skipped"
    fi
}

validate_desktop_file() {
    local tmpdir tmp_desktop

    command -v desktop-file-validate >/dev/null 2>&1 ||
        die "desktop-file-validate is required for desktop metadata validation"
    tmpdir="$(mktemp -d)"
    tmp_desktop="$tmpdir/$app_id.desktop"
    cp "$desktop_file" "$tmp_desktop"
    desktop-file-validate "$tmp_desktop"
    rm -rf "$tmpdir"
}

validate_flatpak_build() {
    local build_dir

    command -v flatpak-builder >/dev/null 2>&1 ||
        die "flatpak-builder is required for release Flatpak validation"
    if [[ "${LUSHTEXT_RELEASE_SKIP_FLATPAK_BUILD:-0}" == "1" ]]; then
        warn "Skipping Flatpak build because LUSHTEXT_RELEASE_SKIP_FLATPAK_BUILD=1"
        return 0
    fi

    build_dir="${LUSHTEXT_RELEASE_FLATPAK_BUILD_DIR:-build-flatpak-release}"
    flatpak-builder \
        --disable-rofiles-fuse \
        --force-clean \
        --user \
        --install-deps-from="${FLATPAK_REMOTE:-flathub}" \
        "$build_dir" \
        "$local_flatpak_manifest"
}

validate_release_metadata() {
    verify_version_surfaces "$1"
    verify_cargo_sources_current
    validate_appstream
    validate_desktop_file
}

prepare_release_files() {
    local version="$1"
    local notes_file="$2"
    local dry_run="${3:-}"
    local plain

    plain="$(plain_version "$version")"

    if [[ "$dry_run" == "1" ]]; then
        echo "Would update version surfaces to $plain:"
        printf '  - %s\n' "${release_files[@]}"
        if [[ -n "$notes_file" ]]; then
            echo "Would read release notes from: $notes_file"
        else
            warn "No RELEASE_NOTES_FILE provided; a real release would fail before committing."
        fi
        echo "Would regenerate or verify $cargo_sources_file"
        return 0
    fi

    [[ -n "$notes_file" ]] ||
        die "RELEASE_NOTES_FILE is required for a real release"
    [[ -f "$notes_file" ]] ||
        die "RELEASE_NOTES_FILE does not exist: $notes_file"

    set_meson_version "$plain"
    set_first_toml_package_version crates/lushtext/Cargo.toml "$plain"
    set_first_toml_package_version crates/lushtext-core/Cargo.toml "$plain"
    refresh_cargo_lock
    upsert_appstream_release "$plain" "$notes_file"
    refresh_cargo_sources
    verify_version_surfaces "$version"
}

check_release_prerequisites() {
    local version="$1"
    local branch

    branch="$(git rev-parse --abbrev-ref HEAD)"
    [[ "$branch" == "main" ]] ||
        die "releases must be created from main (currently on $branch)"

    if [[ -n "$(git status --porcelain)" ]]; then
        die "working tree is dirty; commit or stash changes before releasing"
    fi

    if git rev-parse "refs/tags/$version" >/dev/null 2>&1; then
        die "tag '$version' already exists"
    fi

    git ls-remote --exit-code origin >/dev/null 2>&1 ||
        die "cannot reach remote 'origin'"
}

tag_exists() {
    git rev-parse "refs/tags/$1" >/dev/null 2>&1
}

ensure_staged_files_allowed() {
    local staged path allowed
    mapfile -t staged < <(git diff --cached --name-only)

    [[ "${#staged[@]}" -gt 0 ]] ||
        die "release preparation produced no staged changes"

    for path in "${staged[@]}"; do
        allowed=0
        for expected in "${release_files[@]}"; do
            if [[ "$path" == "$expected" ]]; then
                allowed=1
                break
            fi
        done
        [[ "$allowed" == "1" ]] ||
            die "unexpected staged release file: $path"
    done
}

confirm_release() {
    local version="$1"
    local yes="${2:-}"

    [[ "$yes" == "1" ]] && return 0

    printf 'Create release commit, signed tag %s, and push to origin? [y/N] ' "$version"
    read -r answer
    [[ "$answer" =~ ^[Yy]$ ]] || {
        info "Aborted."
        exit 0
    }
}

print_release_summary() {
    local version="$1"
    local dry_run="${2:-}"
    local head_sha

    head_sha="$(git rev-parse --short HEAD 2>/dev/null || printf 'unknown')"

    printf '\n%s=== Release Summary ===%s\n' "$bold" "$reset"
    printf '  Version: %s%s%s\n' "$bold" "$version" "$reset"
    printf '  Current commit: %s (%s)\n' "$head_sha" "$(git log -1 --format='%s' HEAD 2>/dev/null || printf 'unknown')"
    printf '  Commit message: chore(release): %s\n' "$version"
    printf '  Tag message: Release %s\n' "$version"
    printf '  Remote: origin\n'
    if [[ "$dry_run" == "1" ]]; then
        printf '  Mode: dry run; no files, commits, tags, or remotes will change\n'
    fi
    printf '\n'
}

print_cominotti_flatpak_plan() {
    local version="$1"
    local out_dir="${COMINOTTI_FLATPAK_OUT_DIR:-build-aux/cominotti-flatpak}"
    local repo_url="${COMINOTTI_FLATPAK_REPO_URL:-https://flatpak.cominotti.dev/repo/}"
    local base_url="${COMINOTTI_FLATPAK_BASE_URL:-https://flatpak.cominotti.dev}"
    local deploy_target="${COMINOTTI_FLATPAK_DEPLOY_TARGET:-$base_url/}"
    local public_key="${COMINOTTI_FLATPAK_PUBLIC_KEY:-}"
    local signing_key="${COMINOTTI_FLATPAK_GPG_KEY:-}"
    local cloudflare_project="${COMINOTTI_FLATPAK_CLOUDFLARE_PAGES_PROJECT:-cominotti-sw-flatpak}"
    local pages_max_file_bytes="${COMINOTTI_FLATPAK_PAGES_MAX_FILE_BYTES:-26214400}"
    local pages_max_files="${COMINOTTI_FLATPAK_PAGES_MAX_FILES:-20000}"

    printf 'Would prepare Cominotti Flatpak repository publication:\n'
    printf '  - release tag: %s\n' "$version"
    printf '  - output directory: %s\n' "$out_dir"
    printf '  - repository URL: %s\n' "$repo_url"
    printf '  - repository descriptor: %s/cominotti.flatpakrepo\n' "${base_url%/}"
    printf '  - LushText installer: %s/lushtext.flatpakref\n' "${base_url%/}"
    printf '  - deploy target: %s\n' "$deploy_target"
    printf '  - default hosted backend: Cloudflare Pages project %s\n' "$cloudflare_project"
    printf '  - Cloudflare Pages preflight: max asset %s bytes, max files %s\n' "$pages_max_file_bytes" "$pages_max_files"
    printf '  - fallback when Pages limits are exceeded: Cloudflare R2 behind flatpak.cominotti.dev\n'
    if [[ -n "$public_key" ]]; then
        printf '  - public GPG key file: %s\n' "$public_key"
    else
        printf '  - public GPG key file: not configured; public metadata generation would fail until configured\n'
    fi
    if [[ -n "$signing_key" ]]; then
        printf '  - signing key ID: %s\n' "$signing_key"
    else
        printf '  - signing key ID: not configured; repository signing/deploy would be skipped or fail until configured\n'
    fi
    printf 'Would keep Flathub handoff optional and report it separately from Cominotti publication.\n'
}

create_release_commit_and_tag() {
    local version="$1"

    git add -- "${release_files[@]}"
    ensure_staged_files_allowed
    git commit -m "chore(release): $version"
    git tag -s "$version" -m "Release $version"
    git push origin HEAD:main
    git push origin "$version"
}

run_release() {
    local version="$1"
    local yes="${2:-}"
    local dry_run="${3:-}"
    local notes_file="${RELEASE_NOTES_FILE:-}"

    validate_semver "$version"

    if tag_exists "$version"; then
        die "tag '$version' already exists"
    fi

    print_release_summary "$version" "$dry_run"

    if [[ "$dry_run" == "1" ]]; then
        prepare_release_files "$version" "$notes_file" "$dry_run"
        echo "Would run validation gates:"
        echo "  - release prerequisites"
        echo "  - version surface consistency"
        echo "  - cargo-sources freshness"
        echo "  - AppStream and desktop metadata validation"
        echo "  - Flatpak release build"
        print_cominotti_flatpak_plan "$version"
        echo "Would create commit: chore(release): $version"
        echo "Would create signed tag: $version"
        echo "Would push main and $version to origin"
        return 0
    fi

    check_release_prerequisites "$version"
    confirm_release "$version" "$yes"
    prepare_release_files "$version" "$notes_file" "$dry_run"
    validate_release_metadata "$version"
    validate_flatpak_build
    create_release_commit_and_tag "$version"

    ok "Release $version tagged and pushed."
}

validate_tag_release() {
    local version="$1"
    local skip_flatpak="${2:-}"

    validate_semver "$version"
    validate_release_metadata "$version"
    if [[ "$skip_flatpak" != "1" ]]; then
        validate_flatpak_build
    fi
    ok "Release artifacts validate for $version."
}

main() {
    local mode="${1:-}"

    case "$mode" in
        tag)
            local version="${2:-}"
            local yes="${3:-}"
            local dry_run="${4:-}"

            [[ -n "$version" ]] || die "VERSION is required"
            run_release "$version" "$yes" "$dry_run"
            ;;
        bump)
            local bump_type="${2:-}"
            local prerelease="${3:-}"
            local promote="${4:-}"
            local yes="${5:-}"
            local dry_run="${6:-}"

            [[ -n "$bump_type" ]] || die "TYPE is required"
            compute_next_version "$bump_type" "$prerelease" "$promote"
            run_release "$next_version" "$yes" "$dry_run"
            ;;
        validate-tag)
            local version="${2:-}"
            local skip_flatpak="${3:-}"

            [[ -n "$version" ]] || die "version is required"
            validate_tag_release "$version" "$skip_flatpak"
            ;;
        *)
            die "unknown mode '$mode' (expected tag, bump, or validate-tag)"
            ;;
    esac
}

if [[ "${LUSHTEXT_RELEASE_LIB_ONLY:-0}" == "1" ]]; then
    return 0 2>/dev/null || exit 0
fi

main "$@"
