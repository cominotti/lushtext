# Local container runs L1–L3 (design D8), stage 3, 2026-11-14

All runs used the real CI image on the developer host:

- **Image:** `ghcr.io/flathub-infra/flatpak-github-actions:gnome-51`, id
  `sha256:78d969b18225ae107ca29497bf09c4a3f791e5f1838a9a912a529d0024fb6210`,
  created `2026-09-22T13:43:09Z`. `docker pull` on 2026-11-14 reported
  "Image is up to date", so this is the image CI pulls today.
- **Host:** kernel `7.2.15-300.fc45.x86_64`, overlay `redirect_dir` = `N`, Docker 29
  with the containerd overlayfs snapshotter.
- **Container setup:** `docker run -d --privileged --cpuset-cpus=8-23 --memory=16g
  --memory-swap=16g`, the checkout bound **read-only** at `/__w/lushtext/lushtext`,
  one named Docker volume per container at `/__w/_temp`, and
  `GITHUB_WORKSPACE=/__w/lushtext/lushtext`, `RUNNER_TEMP=/__w/_temp`. Every step
  below is its own `docker exec`, as GitHub runs steps. Containers and volumes
  were removed after each run.
- **Tools in the image:** `bash`, `python3` 3.13.15, `findmnt`, `cp`, `df`, `du`;
  Flatpak 1.18.1, flatpak-builder 1.4.9.

## L1: as shipped (must fail)

Helper `scripts/ci-flatpak-installation.py` sha256 `53c01659…`, the version committed with this L1 record.

The image still lags Flathub on all five refs (`flatpak --system remote-ls --updates`):

```
Flatpak 1.18.1
flatpak-builder-1.4.9
default	org.freedesktop.Platform.GL.default		26.08	x86_64
default	org.freedesktop.Platform.GL.default		26.08-extra	x86_64
codecs-extra	org.freedesktop.Platform.codecs-extra		26.08-extra	x86_64
gnome platform	org.gnome.Platform		51	x86_64
gnome development platform	org.gnome.Sdk		51	x86_64
org.freedesktop.Platform.GL.default/x86_64/26.08	d7085a3efea0
org.freedesktop.Platform.GL.default/x86_64/26.08-extra	e73437ebaa1a
org.freedesktop.Platform.codecs-extra/x86_64/26.08-extra	1ac8ac60ab18
org.gnome.Platform/x86_64/51	23a63567d1ea
org.gnome.Sdk/x86_64/51	9c771940fcbe
```

`diagnose` (excerpt; the overlay lines of `/proc/mounts` and `findmnt` options elided):

```
::group::Flatpak installation diagnostics
kernel: 7.2.15-300.fc45.x86_64
overlay redirect_dir: N
runner image: not visible in the container (see the job's Runner Image header)
$ findmnt -T /var/lib/flatpak
TARGET SOURCE  FSTYPE  OPTIONS
/      overlay overlay rw,relatime,seclabel,lowerdir=…
$ findmnt -T /__w/_temp
TARGET     SOURCE                                                FSTYPE OPTIONS
/__w/_temp /dev/dm-1[/var/lib/docker/volumes/hfsu-l1-temp/_data] btrfs  rw,noatime,…
pre-relocation rename-probe target: /var/lib/flatpak/runtime/org.freedesktop.Platform.GL.default/x86_64/26.08/d7085a3efea0de5516128484a291d72d0c422de626af17ded180b74ec3ffe167
rename-probe: EXDEV
::endgroup::
```

`flatpak-builder --install-deps-only --install-deps-from=flathub --disable-rofiles-fuse
--state-dir "$RUNNER_TEMP/state" "$RUNNER_TEMP/build" <manifest>` (progress bars removed):

```
Dependency Sdk: org.gnome.Sdk 51
Updating org.gnome.Sdk/x86_64/51
Updating runtime/org.freedesktop.Platform.GL.default/x86_64/26.08
Warning: Failed to update org.freedesktop.Platform.GL.default: Invalid cross-device link
Updating runtime/org.freedesktop.Platform.GL.default/x86_64/26.08-extra
Warning: Failed to update org.freedesktop.Platform.GL.default: Invalid cross-device link
Installing runtime/org.freedesktop.Platform.VAAPI.Intel/x86_64/26.08
Updating runtime/org.freedesktop.Platform.codecs-extra/x86_64/26.08-extra
Warning: Failed to update org.freedesktop.Platform.codecs-extra: Invalid cross-device link
Updating runtime/org.gnome.Sdk.Locale/x86_64/51
Warning: Failed to update org.gnome.Sdk.Locale: Invalid cross-device link
Updating runtime/org.gnome.Sdk/x86_64/51
Error: Failed to update org.gnome.Sdk: Invalid cross-device link
flatpak-builder exit=139
```

L1 holds: `overlay`, `rename-probe: EXDEV`, every update fails with
`Invalid cross-device link` (the one fresh install, `VAAPI.Intel`, needs no rename and
succeeds), and flatpak-builder then segfaults (exit 139), exactly as in CI run
`43642169956` attempts 1 and 2.

## L2: relocated (must pass)

Fresh container `hfsu-l2`, same image and setup, helper sha256 `53c01659…`
(unchanged since L1). The source for step 5 is `git archive HEAD` of commit
`6b931da03e6f52d084c60b5ecf63165433c9346b`, written **on the host** and bound read-only
into the container as a tar file: the checkout is a git worktree whose `.git`
points at a host path the container cannot see, so `git archive` inside the
container (task 3.4's first form) cannot run. The tar carries only the committed
tree, as CI's clean checkout does (no `target/`, no untracked file). The
workflow edits of this commit are not in it; they do not touch build inputs.

1. `diagnose` (separate exec): `overlay redirect_dir: N`, `/var/lib/flatpak` on
   `overlay`, `rename-probe: EXDEV` (target the `GL.default//26.08` commit
   `d7085a3e…`).
2. `relocate` (separate exec):

```
::group::Relocate the system Flatpak installation off overlayfs
relocating: /var/lib/flatpak is on overlay, whose lower-layer directories may not be renamable
$ df -h /var/lib/flatpak /__w/_temp /tmp
Filesystem      Size  Used Avail Use% Mounted on
overlay         1.5T  831G  701G  55% /
/dev/dm-1       1.5T  831G  701G  55% /__w/_temp
overlay         1.5T  831G  701G  55% /
$ cp -a /var/lib/flatpak /__w/_temp/flatpak-system
/var/lib/flatpak is now on: btrfs /dev/dm-1[/var/lib/docker/volumes/hfsu-l2-temp/_data/flatpak-system] /var/lib/flatpak
post-relocation rename-probe target: /var/lib/flatpak/runtime/org.freedesktop.Platform.GL.default/x86_64/26.08/d7085a3efea0de5516128484a291d72d0c422de626af17ded180b74ec3ffe167
rename-probe: ok
$ du -sh /__w/_temp/flatpak-system
3.8G	/__w/_temp/flatpak-system
$ df -h /var/lib/flatpak /__w/_temp /tmp
Filesystem      Size  Used Avail Use% Mounted on
/dev/dm-1       1.5T  834G  698G  55% /var/lib/flatpak
/dev/dm-1       1.5T  834G  698G  55% /__w/_temp
overlay         1.5T  834G  698G  55% /
relocation wall time: 21.0 s (copy 21.0 s, mount 0.000 s, probe 0.003 s; budget 90 s, judged from CI logs, not enforced)
::endgroup::
```

   The 21.0 s is **non-verdict**: the host was under other agents' load (1-min
   load about 9–13), the page cache was not controlled, and it was not run
   through `scripts/measure.sh` (it is a CI cost figure, not a responsiveness
   measurement; design D2). L3's copy of the same tree took 12.2 s, the stage 2
   reproduction 6.8 s.
3. `findmnt -T /var/lib/flatpak` (separate exec), the mount survives:

```
TARGET           SOURCE                                                               FSTYPE OPTIONS
/var/lib/flatpak /dev/dm-1[/var/lib/docker/volumes/hfsu-l2-temp/_data/flatpak-system] btrfs  rw,noatime,seclabel,degraded,compress=zstd:1,ssd,flushoncommit,discard=async,space_cache=v2,subvolid=257,subvol=/var
```

4. The same `flatpak-builder --install-deps-only` as L1 (separate exec), progress
   bars removed:

```
Dependency Sdk: org.gnome.Sdk 51
Updating org.gnome.Sdk/x86_64/51
Updating runtime/org.freedesktop.Platform.GL.default/x86_64/26.08
Updating runtime/org.freedesktop.Platform.GL.default/x86_64/26.08-extra
Installing runtime/org.freedesktop.Platform.VAAPI.Intel/x86_64/26.08
Updating runtime/org.freedesktop.Platform.codecs-extra/x86_64/26.08-extra
Updating runtime/org.gnome.Sdk.Locale/x86_64/51
Updating runtime/org.gnome.Sdk/x86_64/51
Dependency Runtime: org.gnome.Platform 51
Updating org.gnome.Platform/x86_64/51
Updating runtime/org.gnome.Platform.Locale/x86_64/51
Updating runtime/org.gnome.Platform/x86_64/51
Dependency Extension: org.freedesktop.Sdk.Extension.rust-stable 26.08
Installing org.freedesktop.Sdk.Extension.rust-stable/x86_64/26.08 from flathub
Installing runtime/org.freedesktop.Sdk.Extension.rust-stable/x86_64/26.08
flatpak-builder exit=0
org.freedesktop.Platform.GL.default/x86_64/26.08	5aec1b8281d5
org.freedesktop.Platform.GL.default/x86_64/26.08-extra	4d87aeea9f59
org.freedesktop.Platform.VAAPI.Intel/x86_64/26.08	5c21d37862ee
org.freedesktop.Platform.codecs-extra/x86_64/26.08-extra	2b9f95bc563d
org.freedesktop.Sdk.Extension.rust-stable/x86_64/26.08	14774cebefea
org.gnome.Platform/x86_64/51	2ec125d2f324
org.gnome.Sdk/x86_64/51	f0bca3a94eda
```

   Every ref L1 failed on updates; `codecs-extra` moves `1ac8ac60` →
   `2b9f95bc`, `org.gnome.Sdk` `9c771940` → `f0bca3a9`.

## L3: refusals (must refuse)

Fresh container `hfsu-l3`, same image and setup, helper sha256 `53c01659…`.

**A destination inside the checkout** (`RUNNER_TEMP=/__w/lushtext/lushtext/_temp`
for this exec only) is refused before any copy; the destination was never
created and `/var/lib/flatpak` stays on `overlay`:

```
::group::Relocate the system Flatpak installation off overlayfs
::endgroup::
::error::relocate: refusing to relocate: destination /__w/lushtext/lushtext/_temp/flatpak-system lies inside GITHUB_WORKSPACE /__w/lushtext/lushtext; the manifest's `dir` source would read the copy as build input
relocate exit=1
ls: cannot access '/__w/lushtext/lushtext/_temp': No such file or directory
overlay
```

(With the checkout bound read-only the copy could not have landed there anyway;
the refusal comes from the plan, before `cp` runs, which is what protects a
writable CI checkout.)

**A forced post-relocation probe failure.** The helper's failure-only hook
`CI_FLATPAK_FORCE_PROBE_FAILURE=EROFS` (it rejects `ok` and unknown names, and
only the post-relocation probe honours it; self-tested) makes the re-probe
report `EROFS` after a real copy and bind mount:

```
::group::Relocate the system Flatpak installation off overlayfs
relocating: /var/lib/flatpak is on overlay, whose lower-layer directories may not be renamable
$ df -h /var/lib/flatpak /__w/_temp /tmp
Filesystem      Size  Used Avail Use% Mounted on
overlay         1.5T  838G  694G  55% /
/dev/dm-1       1.5T  838G  694G  55% /__w/_temp
overlay         1.5T  838G  694G  55% /
$ cp -a /var/lib/flatpak /__w/_temp/flatpak-system
/var/lib/flatpak is now on: btrfs /dev/dm-1[/var/lib/docker/volumes/hfsu-l3-temp/_data/flatpak-system] /var/lib/flatpak
post-relocation rename-probe target: /var/lib/flatpak/runtime/org.freedesktop.Platform.GL.default/x86_64/26.08/d7085a3efea0de5516128484a291d72d0c422de626af17ded180b74ec3ffe167
rename-probe: EROFS
$ du -sh /__w/_temp/flatpak-system
3.8G	/__w/_temp/flatpak-system
$ df -h /var/lib/flatpak /__w/_temp /tmp
Filesystem      Size  Used Avail Use% Mounted on
/dev/dm-1       1.5T  841G  691G  55% /var/lib/flatpak
/dev/dm-1       1.5T  841G  691G  55% /__w/_temp
overlay         1.5T  841G  691G  55% /
relocation wall time: 12.2 s (copy 12.2 s, mount 0.000 s, probe 0.003 s; budget 90 s, judged from CI logs, not enforced)
::endgroup::
::error::relocate: post-relocation rename-probe returned EROFS, not ok; /var/lib/flatpak is on btrfs /dev/dm-1[/var/lib/docker/volumes/hfsu-l3-temp/_data/flatpak-system] /var/lib/flatpak. flatpak-builder would fail to update dependencies
relocate exit=1
```

The step fails before flatpak-builder could start, naming the probe, the result
(`EROFS`) and the filesystem under `/var/lib/flatpak`. A later `diagnose` in the
same container with the hook set to `ok` ran the real probe (`rename-probe: ok`),
so the hook never reaches the diagnostic probe and never forces a pass.

## Release dry-run path (task 3.3)

In a scratch clone of the branch (commit `6b931da0`), with the tag
`v0.8.4-dryrun.1` absent locally and on `origin` (`git ls-remote --tags`):

- `make release VERSION=v0.8.4-dryrun.1 DRY_RUN=1`: exit 0 ("Would create commit:
  chore(release): v0.8.4-dryrun.1", "Would create signed tag", "Would push main
  and v0.8.4-dryrun.1 to origin"), nothing mutated.
- The old step body's `git rev-parse -q --verify 'v0.8.4-dryrun.1^{commit}'`:
  exit 1, the pre-existing blocker.
- The new `Generate Flathub update artifact` body, extracted from `release.yml`
  and run with `VERSION=v0.8.4-dryrun.1 DRY_RUN=true`: exit 0, "Flathub manifest
  is valid", and the generated manifest's source `commit` is `HEAD`
  (`6b931da03e6f…`). With `VERSION=v0.8.3` (tagged) it takes the tag branch.
- `git status --short` in the clone afterwards: empty.
- The new `Determine version` body, extracted and run with each event:

| Event | `dry_run` | `GITHUB_REF` | Exit | Outputs |
|---|---|---|---|---|
| `workflow_dispatch` | false | `refs/heads/feature` | 1, `::error::… must come from main; it was dispatched from refs/heads/feature` | none |
| `workflow_dispatch` | false | `refs/heads/main` | 0 | `checkout-ref=$GITHUB_SHA` |
| `workflow_dispatch` | true | `refs/heads/feature` | 0 | `dry-run=true`, `checkout-ref=$GITHUB_SHA` |
| `push` (tag) | — | `refs/tags/v0.8.4` | 0 | `checkout-ref=v0.8.4` |

`actionlint` passes on `flatpak.yml`, `release.yml` and `ci.yml`.
