# Changelog

## Unreleased

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
