#!/usr/bin/env bash
# Stage-2 review experiment for stable-flatpak-install-origin: the rejected
# alternative of pointing the origin at an EMPTY archive repository with a
# summary before the build. Private FLATPAK_USER_DIR only.
set -uo pipefail
S=$(mktemp -d "${TMPDIR:-/tmp}/fp-empty-XXXXXX"); trap 'rm -rf "$S"' EXIT
export FLATPAK_USER_DIR=$S/user
case $FLATPAK_USER_DIR in "$S"/*) ;; *) echo refuse; exit 2;; esac
R=org.test.lushtext/x86_64/master
mk() { rm -rf $S/d; mkdir -p $S/d/usr $S/d/files $S/d/var; printf '[Runtime]\nname=org.test.lushtext\n' > $S/d/metadata; echo "$2" > $S/d/usr/v; flatpak build-export --runtime "$1" $S/d master >/dev/null; }
mk $S/old one; flatpak install --user -y --noninteractive --reinstall $S/old $R >/dev/null 2>&1; rm -rf $S/old
flatpak remote-modify --user --disable lushtext-origin
mkdir -p $S/shared; ostree init --mode=archive-z2 --repo=$S/shared/repo; flatpak build-update-repo $S/shared/repo >/dev/null; echo "empty repo: $(ls $S/shared/repo | tr '\n' ' ')"
flatpak remote-modify --user --enable --url=file://$S/shared/repo lushtext-origin
flatpak remote-ls --user --updates >/dev/null 2>$S/e; echo "remote-ls --updates rc=$? $(head -c 300 $S/e)"
flatpak update --user --noninteractive >$S/u 2>&1; echo "update rc=$? $(grep -i -E 'error|warn' $S/u | head -2)"
mk $S/shared/repo two; head=$(ostree --repo=$S/shared/repo rev-parse runtime/$R)
flatpak install --user -y --noninteractive --reinstall $S/shared/repo $R >/dev/null 2>&1; echo "install rc=$?"
flatpak remotes --user --show-disabled --columns=name,url,options | sed "s|$S|\$S|g"
[ "$head" = "$(flatpak info --user -c $R)" ] && echo "deployed == head" || echo STALE
