#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later

set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd -- "$script_dir/.." && pwd)"
tmpdir="$(mktemp -d)"

cleanup() {
    rm -rf "$tmpdir"
}
trap cleanup EXIT

mkdir -p "$tmpdir/bin" \
    "$tmpdir/xdg/flatpak/exports/share/applications" \
    "$tmpdir/system-exports"

cat > "$tmpdir/bin/flatpak" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

if [[ "$1" == "info" && "$2" == "--show-metadata" && "$3" == "dev.cominotti.lushtext" ]]; then
    cat <<'META'
[Application]
name=dev.cominotti.lushtext
runtime=org.gnome.Platform/x86_64/50
sdk=org.gnome.Sdk/x86_64/50
command=lushtext
META
    exit 0
fi

if [[ "$1" == "info" && "$2" == "--show-permissions" && "$3" == "dev.cominotti.lushtext" ]]; then
    cat <<'PERMS'
[Context]
shared=ipc;
sockets=fallback-x11;wayland;
devices=dri;
filesystems=home;
PERMS
    exit 0
fi

echo "unexpected flatpak invocation: $*" >&2
exit 1
EOF
chmod +x "$tmpdir/bin/flatpak"

cat > "$tmpdir/bin/gio" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

if [[ "$1" == "mime" ]]; then
    cat <<'MIME'
Default application: org.gnome.TextEditor.desktop
Registered applications:
	dev.cominotti.lushtext.desktop
Recommended applications:
	dev.cominotti.lushtext.desktop
MIME
    exit 0
fi

echo "unexpected gio invocation: $*" >&2
exit 1
EOF
chmod +x "$tmpdir/bin/gio"

export PATH="$tmpdir/bin:$PATH"
export XDG_DATA_HOME="$tmpdir/xdg"
export LUSHTEXT_SYSTEM_FLATPAK_EXPORT_DIR="$tmpdir/system-exports"

cat > "$tmpdir/xdg/flatpak/exports/share/applications/dev.cominotti.lushtext.desktop" <<'EOF'
[Desktop Entry]
Name=LushText
Exec=/usr/bin/flatpak run dev.cominotti.lushtext
Type=Application
X-Flatpak=dev.cominotti.lushtext
EOF

"$repo_root/scripts/verify-flatpak-identity.sh" > "$tmpdir/success.log"
grep -q "Flatpak desktop identity is usable" "$tmpdir/success.log"

mkdir -p "$tmpdir/xdg/applications"
cat > "$tmpdir/xdg/applications/dev.cominotti.lushtext.desktop" <<'EOF'
[Desktop Entry]
Name=LushText
Exec=/tmp/lushtext
Type=Application
EOF

if "$repo_root/scripts/verify-flatpak-identity.sh" > "$tmpdir/fail.log" 2>&1; then
    echo "expected verifier to fail for a same-ID non-Flatpak desktop entry" >&2
    exit 1
fi
grep -q "same-ID non-Flatpak desktop entry" "$tmpdir/fail.log"

echo "flatpak identity verifier tests passed"
