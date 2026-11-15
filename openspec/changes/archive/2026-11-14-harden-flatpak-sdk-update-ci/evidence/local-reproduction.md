# Local reproduction (stage 2 review, 2026-11-14)

Host: Fedora 45, kernel `7.2.15-300.fc45.x86_64`, `/sys/module/overlay/parameters/redirect_dir` = `N`.
Docker 29.9.2, storage driver `overlayfs` through `io.containerd.snapshotter.v1` (the containerd image store).
Image: `ghcr.io/flathub-infra/flatpak-github-actions:gnome-51`, image id
`sha256:78d969b18225ae107ca29497bf09c4a3f791e5f1838a9a912a529d0024fb6210`, created `2026-09-22T13:43:09Z`.
Every container ran `--privileged`, pinned with `taskset -c 8-23`. Nothing was written into the checkout.
A named Docker volume stood in for `$RUNNER_TEMP`, so no root-owned files were left behind.

## What the image ships

| Tool | Present |
|---|---|
| `findmnt`, `stat`, `du`, `mountpoint`, `unshare`, `nsenter` | yes (`/usr/sbin`) |
| GNU `cp` | yes (coreutils 9.7) |
| `python3` (with `ctypes`, `libc.so.6`), `perl` | yes |
| `mount`, `umount`, `busybox` | **no** |

- The image is the Freedesktop SDK 25.08 runtime.
- `/var/lib/flatpak` is on `overlay` and takes `3.8G` (`du -sh`).
- System installation: `org.gnome.Sdk//51` `9c771940fcbe` (2.5 GB), `org.gnome.Platform//51` `23a63567d1ea`, `GL.default` 26.08 and 26.08-extra, `codecs-extra` 26.08-extra `1ac8ac60ab18`.
- `flatpak --system remote-ls --updates` lists all five as updatable. The image lags Flathub on every ref that CI run `43642169956` failed to update.

## As shipped: the CI failure reproduces

```
rename-probe target: /var/lib/flatpak/runtime/org.freedesktop.Platform.codecs-extra/x86_64/26.08-extra/1ac8ac60…
rename-probe: EXDEV
$ flatpak update --system -y --noninteractive org.freedesktop.Platform.codecs-extra//26.08-extra
Updating runtime/org.freedesktop.Platform.codecs-extra/x86_64/26.08-extra
Error: Failed to update org.freedesktop.Platform.codecs-extra: Invalid cross-device link
exit=1
```

The error text is the one in CI job `132591925079`. The probe ran `os.rename` of the deployed commit directory, a lower-layer directory, to a sibling name. That returned EXDEV, and the update failed the same way.

## Relocated: the update succeeds

The container was started once with `docker run -d`, and each of the following was a separate `docker exec`. GitHub runs each step as a separate exec, so this also tests whether the mount survives from one step to the next.

1. Exec 1:
   - `cp -a /var/lib/flatpak /__w/_temp/flatpak-system` took 6.8 s. This is a non-verdict figure: NVMe, page cache not controlled, not run through `scripts/measure.sh`.
   - The copy measured `3.8G`, the same size as the source, so `cp -a` kept the hardlinks.
   - `mount(2)` with `MS_BIND`, called through `python3` `ctypes`, returned 0.
2. Exec 2:
   - `findmnt -T /var/lib/flatpak` reported `btrfs /dev/dm-1[/var/lib/docker/volumes/…/flatpak-system]`, so the mount survived into the next exec.
   - `rename-probe: ok`.
   - `flatpak update --system -y org.freedesktop.Platform.codecs-extra//26.08-extra` succeeded, and the active commit moved from `1ac8ac60ab18` to `2b9f95bc563d`.

## Unrelated small image, same host

`rename` through `perl` of `/usr/share/apt` in another overlay container returned `Invalid cross-device link`. The same rename returned `ok` after `cp -a` plus a bind mount onto a host-backed path. So the effect comes from the overlay mount, not from flatpak.

## What this does and does not settle

- **Settled:** with overlay `redirect_dir` off, a directory rename inside the image's `/var/lib/flatpak` returns EXDEV, and flatpak's update fails with the exact CI error. Copying the installation (hardlinks kept) and bind-mounting the copy over it makes both the probe and the update succeed, and the mount persists across execs.
- **Not settled:** why attempt 3 on runner image `20261111.196.1` updated successfully. That runner's overlay may have `redirect_dir` on, or it may use a different snapshotter. The fix does not depend on the answer. The always-on diagnostics will record it at the programme's final CI sweep.
