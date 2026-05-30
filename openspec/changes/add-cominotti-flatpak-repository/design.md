## Context

LushText already ships a Flatpak build through `build-aux/dev.cominotti.lushtext.Flatpak.json`, Meson, vendored Cargo sources, AppStream metadata, and release validation. The current publication automation is shaped around a Flathub manifest pull request, but Flathub's generative-AI submission policy makes that path unreliable as the primary release channel for this project.

Flatpak itself does not require Flathub or a specific Pages provider. A remote is an OSTree repository that can be served from any reliable static HTTP location. The Cominotti-owned remote should therefore live under `flatpak.cominotti.dev`, expose install metadata at stable URLs, and reuse Flathub only as the runtime source for `org.gnome.Platform` and `org.gnome.Sdk`.

## Goals / Non-Goals

**Goals:**
- Publish LushText as the first app in a shared Cominotti Flatpak remote.
- Use `cominotti` as the user-facing remote name and `dev.cominotti.Apps` as the collection identity.
- Generate signed repository metadata, static deltas, `.flatpakrepo`, and `lushtext.flatpakref` from release automation.
- Keep the hosting target provider-neutral: any deploy path that serves `https://flatpak.cominotti.dev/...` is acceptable.
- Preserve the existing LushText Flatpak packaging contract: app ID, runtime, SDK, finish args, Meson release profile, and vendored Cargo sources.
- Keep optional Flathub PR generation from blocking Cominotti publication.

**Non-Goals:**
- Building a full app store UI or replacing GNOME Software's discovery model.
- Moving the LushText source tree into a separate Flatpak repository.
- Publishing Cominotti-owned GNOME runtimes or SDKs.
- Tightening LushText's current `--filesystem=host` permission posture in this change.
- Requiring a specific hosting backend, while still preferring Cloudflare Pages for bandwidth-sensitive static asset delivery.

## Decisions

### D1: Publisher-level remote, app-specific source repos

The Flatpak remote is publisher-owned (`cominotti`), while app source remains app-owned (`cominotti/lushtext`). The remote can contain multiple app refs over time, but this change only publishes `dev.cominotti.lushtext`.

Alternative: create a `lushtext`-only remote. That is simpler for the first release but creates a new trust prompt and remote for every future Cominotti app.

### D2: Host under `flatpak.cominotti.dev`

The public layout is:

```text
https://flatpak.cominotti.dev/repo/
https://flatpak.cominotti.dev/cominotti.flatpakrepo
https://flatpak.cominotti.dev/lushtext.flatpakref
```

The implementation stages these files and directories without coupling to a specific static-hosting provider. The deploy mechanism can be a VPS, object storage, CDN-backed bucket, or any other host that preserves static files and HTTPS.

Alternative: use the existing `cominotti.dev/flatpak/` website path. That is simpler if the current Netlify website deploy owns all static files, but a dedicated subdomain is clearer for user trust and lets the Flatpak repository move independently.

### D3: Signed repository with static deltas

Release publication signs the app commit and repository summary with a configured GPG key, imports the public key into the repository metadata, and generates static deltas. Static deltas trade extra storage for faster installs and updates, which matters because Flatpak repositories otherwise involve many small HTTP requests.

Alternative: distribute unsigned remotes with `--no-gpg-verify`. That is acceptable only for local testing and must not appear in public install instructions.

### D4: `.flatpakref` is the primary install entry point

Users install LushText through `lushtext.flatpakref`, which points at the shared Cominotti remote, includes the GPG public key, names `dev.cominotti.lushtext`, and uses Flathub's `.flatpakrepo` as the runtime repository. Users who prefer manual setup can add `cominotti.flatpakrepo` and then install the app by ID.

Alternative: publish only a `.flatpak` bundle in GitHub Releases. Bundles are useful for smoke testing, but they do not provide the normal remote/update experience.

### D5: Cominotti publication is primary; Flathub is optional

The release workflow should treat Cominotti remote publication as the primary Flatpak publication path. Flathub manifest generation can remain available as a secondary artifact or PR path when explicitly configured, but missing Flathub credentials must not make a Cominotti release incomplete.

Alternative: remove all Flathub tooling immediately. Keeping it optional preserves prior investment and allows a later exception-based submission if policy or project maturity changes.

## Risks / Trade-offs

- [Signing key compromise] -> Keep the private key out of the repository, store it only in protected deployment secrets, and document rotation as a manual recovery step.
- [Static hosting serves stale or partial repo contents] -> Stage into a fresh output directory, validate locally before deploy, and publish atomically where the host supports it.
- [Users distrust a non-Flathub remote] -> Use the first-party `flatpak.cominotti.dev` domain, GPG verification, clear install docs, and no `--no-gpg-verify` instructions.
- [Remote grows beyond LushText needs] -> Keep the repo metadata publisher-level, but keep app manifests and release logic app-local until another Cominotti app exists.
- [Flathub runtime dependency unavailable on user systems] -> Include `RuntimeRepo=https://dl.flathub.org/repo/flathub.flatpakrepo` in the `.flatpakref` so runtime setup is explicit.

## Migration Plan

1. Add repo-generation scripts and Make targets that produce a signed local `repo/`, `cominotti.flatpakrepo`, and `lushtext.flatpakref`.
2. Add validation for metadata fields, GPG key presence, collection ID, runtime repo, generated summary, and installability from a local or staged remote.
3. Extend release CI to upload or stage Cominotti publication artifacts from tagged releases.
4. Add deploy documentation for `flatpak.cominotti.dev`, including the expected public URL layout and secret requirements.
5. Update existing Flathub wording so Flathub is presented as optional/secondary, not as the release completion criterion.

## Open Questions

- Which deploy backend will serve `flatpak.cominotti.dev` in production?
- What GPG key lifecycle should be used for the Cominotti Flatpak remote: a dedicated packaging key, hardware-backed key, or CI-only subkey?
- Should the public branch be `stable` from the first release, or should the first publication use a `beta` branch until smoke testing is complete?
