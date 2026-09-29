# Changelog

## Unreleased

- The supervising parent now owns every child's temporary files. It creates
  one `<prefix><pid>-<random>` root in the system temp dir per run, gives each
  child attempt its own empty subdirectory as `TMPDIR`, removes that
  subdirectory when the child exits, and removes the root when the run ends,
  on failing runs too. A child that leaked one fixed-name directory per process
  had accumulated 45,978 directories and exhausted a tmpfs's inodes. New
  `HarnessConfig::with_run_scratch_prefix()` / `run_scratch_prefix()` and
  `DEFAULT_RUN_SCRATCH_PREFIX` (`gtk-lush-proof-run-`) choose the prefix; the
  parent's startup sweep reclaims roots under it (and legacy `<prefix><pid>`
  names) whose PID is gone and which have been idle for an hour, owned by the
  current user only. Additive API; the behaviour change is that children now
  run with a harness-owned `TMPDIR`.
- The headless relaunch's compositor runtime directory is now named
  `gtk-lush-proof-runtime-<pid>-<random>` and is removed only after no process
  still carries it as `XDG_RUNTIME_DIR` (bounded at ten seconds).
  `dbus-run-session` returns while `xdg-document-portal` still holds its FUSE
  mount on `doc/`, so the previous immediate removal failed and leaked the
  directory on every plain `cargo test` run. Stale directories of both the new
  and the legacy PID-less form are swept.

- `recommended_pre_gtk_environment()` now returns **five** settings instead of
  four, adding `GTK_IM_MODULE=gtk-im-context-simple`. Headless Mutter advertises
  `zwp_text_input_manager_v3` with no input method behind it, so GTK enabled its
  Wayland text-input backend for every focused editable and destroying a focused
  entry raced the compositor's reply into a **SIGSEGV** in
  `wl_proxy_get_version`. Measured 9 failures in 40 isolated runs before, 0 in 40
  after. The return type changes from `[RecommendedEnvironment; 4]` to
  `[RecommendedEnvironment; 5]`, which is a breaking signature change for a crate
  that has not published.

## 0.0.0

- First functional in-tree pre-publication implementation for headless GTK
  test harness orchestration and wait helpers.
- Adoption-lab and matrix evidence before functional publication.
