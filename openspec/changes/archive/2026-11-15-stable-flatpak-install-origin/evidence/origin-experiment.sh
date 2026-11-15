#!/usr/bin/env bash
# Throwaway experiment for stable-flatpak-install-origin (stage 1): how
# `flatpak install --user -y --noninteractive --reinstall <repo-path> <ref>`
# (what flatpak-builder --install runs, builder-main.c do_install) treats an
# existing `<last-id-component>-origin` remote. Uses a private FLATPAK_USER_DIR
# and a dependency-free runtime ref org.test.lushtext (same "lushtext" prefix),
# never the real user installation.
set -uo pipefail
S=$(mktemp -d "${TMPDIR:-/tmp}/fp-origin-XXXXXX"); trap 'rm -rf "$S" "$S.link"' EXIT
export FLATPAK_USER_DIR=$S/user
case $FLATPAK_USER_DIR in "$S"/*) ;; *) echo refuse; exit 2;; esac
R=org.test.lushtext/x86_64/master
mk() { rm -rf $S/d; mkdir -p $S/d/usr $S/d/files $S/d/var; printf '[Runtime]\nname=org.test.lushtext\n' > $S/d/metadata; echo "$2" > $S/d/usr/v; echo "$2" > $S/d/files/v; flatpak build-export --runtime "$1" $S/d master >/dev/null; }
inst() { flatpak install --user -y --noninteractive --reinstall "$1" $R >$S/out 2>&1; echo "   install rc=$? $(grep -i -E 'error|warn' $S/out | head -1)"; }
st() { echo "-- $1"; flatpak remotes --user -d --show-disabled | awk -F'\t' -v s="$S" '{gsub(s,"$S",$3); print "   remote:", $1, $3, $8}'; flatpak list --user --runtime --columns=ref,origin | sed 's/^/   deploy: /'; if flatpak remote-ls --user --updates >/dev/null 2>$S/err; then echo "   remote-ls --updates: OK"; else echo "   remote-ls --updates: FAIL: $(sed "s|$S|\$S|g" $S/err)"; fi; }
mk $S/repoA one; mk $S/repoB two; echo "export repo contents: $(ls $S/repoA | tr '\n' ' ')"
inst $S/repoA; st "a: fresh install from A"
inst $S/repoB; st "b: reinstall from B (different URL)"
rm -rf $S/repoB; st "c: repo B deleted (the 2026-11-14 bug)"
flatpak remote-modify --user --disable lushtext1-origin; st "c2: dangling origin disabled (the maintainer machine now)"
inst $S/repoA; st "d: install from A with the dangling disabled origin present"
mk $S/repoA three; head=$(ostree --repo=$S/repoA rev-parse runtime/$R)
flatpak remote-modify --user --disable lushtext-origin; inst $S/repoA; st "e: origin disabled, same URL, new commit in A"
[ "$head" = "$(flatpak info --user -c $R)" ] && echo "   deployed == repo head" || echo "   deployed != repo head: STALE although install exited 0"
flatpak remote-modify --user --enable lushtext-origin; inst $S/repoA; st "f: re-enabled first, same URL"
[ "$head" = "$(flatpak info --user -c $R)" ] && echo "   deployed == repo head" || echo "   STALE"
mk $S/repoB four; flatpak remote-modify --user --url=file://$S/repoB lushtext-origin; inst $S/repoB; st "g: repointed to B first, install from B"
ln -sfn $S $S.link; inst $S.link/repoB; st "h: same repo B through a symlinked path spelling"
for v in 5 6 7; do mk $S/repoB v$v; done
echo "-- i: prune: commits before $(ostree --repo=$S/repoB log runtime/$R | grep -c '^commit')"
flatpak build-update-repo --prune --prune-depth=0 $S/repoB | tail -1 | sed 's/^/   /'
echo "   commits after $(ostree --repo=$S/repoB log runtime/$R | grep -c '^commit')"
inst $S/repoB; st "i: install after prune"
