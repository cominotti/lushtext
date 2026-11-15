#!/usr/bin/env bash
# Stage-2 review experiment for stable-flatpak-install-origin: what the
# origin looks like when `prepare` repoints and enables it BEFORE the build
# (and the build then fails), against exporting first and repointing after.
# Private FLATPAK_USER_DIR only, never the real user installation.
set -uo pipefail
S=$(mktemp -d "${TMPDIR:-/tmp}/fp-order-XXXXXX"); trap 'rm -rf "$S"' EXIT
export FLATPAK_USER_DIR=$S/user
case $FLATPAK_USER_DIR in "$S"/*) ;; *) echo refuse; exit 2;; esac
R=org.test.lushtext/x86_64/master
mk() { rm -rf $S/d; mkdir -p $S/d/usr $S/d/files $S/d/var; printf '[Runtime]\nname=org.test.lushtext\n' > $S/d/metadata; echo "$2" > $S/d/usr/v; flatpak build-export --runtime "$1" $S/d master >/dev/null; }
rls() { if flatpak remote-ls --user --updates >/dev/null 2>$S/err; then echo "   remote-ls --updates: OK"; else echo "   remote-ls --updates: FAIL: $(sed "s|$S|\$S|g" $S/err)"; fi; }
# machine state: dangling disabled lushtext-origin
mk $S/old one; flatpak install --user -y --noninteractive --reinstall $S/old $R >/dev/null 2>&1; rm -rf $S/old
flatpak remote-modify --user --disable lushtext-origin
echo "-- author order: prepare (repoint+enable to not-yet-existing shared repo), then the build fails"
flatpak remote-modify --user --enable --url=file://$S/shared/repo lushtext-origin; rls
flatpak remotes --user --show-disabled --columns=name,url,options | sed "s|$S|\$S|g; s/^/   /"
echo "-- reordered: export first, then prepare, then install"
flatpak remote-modify --user --disable --url=file://$S/gone lushtext-origin
mkdir -p $S/shared; mk $S/shared/repo two; head=$(ostree --repo=$S/shared/repo rev-parse runtime/$R)
flatpak remote-modify --user --enable --url=file://$S/shared/repo lushtext-origin
flatpak install --user -y --noninteractive --reinstall $S/shared/repo $R >$S/out 2>&1; echo "   install rc=$?"
flatpak remotes --user --show-disabled --columns=name,url,options | sed "s|$S|\$S|g; s/^/   /"
[ "$head" = "$(flatpak info --user -c $R)" ] && echo "   deployed == head" || echo "   STALE"
echo "   origin: $(flatpak info --user -o $R)"
echo "   remote-info commit == head: $([ "$(flatpak remote-info --user -c lushtext-origin $R 2>&1)" = "$head" ] && echo yes || echo no)"
rls
