# Release terminology guard

Why `make check-terminology` runs in the local `make check-policy` gate as well
as in the CI test lane. The rule is in
[`.agents/rules/build.md`](../../.agents/rules/build.md), "Dev Builds".

`scripts/release.sh` inserts AppStream release notes into
`data/dev.cominotti.lushtext.metainfo.xml.in`, and `make release` runs no tests,
so a retired-vocabulary phrase in release prose would otherwise fail CI only
after the public tag was pushed. That happened: the v0.7.0 release found the
retired folder-root and note vocabulary in
`openspec/specs/workspace-entry-visibility/spec.md:34`, committed on `main` in
`08a11823`, because the guard ran nowhere local. The test costs under a second
once the test binary is built.
