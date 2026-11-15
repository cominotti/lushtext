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
