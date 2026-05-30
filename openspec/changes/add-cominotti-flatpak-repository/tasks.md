## 1. Publication Artifact Generation

- [x] 1.1 Audit the existing Flathub manifest generator, verifier, Make targets, and release workflow to identify reusable pieces for provider-neutral Cominotti publication.
- [x] 1.2 Add a Cominotti release-manifest or repository-generation path that uses the tagged LushText source, release commit, existing Flatpak manifest invariants, and vendored Cargo sources.
- [x] 1.3 Add generation for `cominotti.flatpakrepo` with remote name `cominotti`, repository URL `https://flatpak.cominotti.dev/repo/`, collection ID `dev.cominotti.Apps`, publisher metadata, and the configured public GPG key.
- [x] 1.4 Add generation for `lushtext.flatpakref` with app ID `dev.cominotti.lushtext`, suggested remote `cominotti`, runtime repo `https://dl.flathub.org/repo/flathub.flatpakrepo`, repository URL, branch, and the configured public GPG key.
- [x] 1.5 Add repository export/update logic that signs the app commit and summary, imports the public key into repo metadata, generates static deltas, and stages artifacts under the documented `flatpak/` URL layout.

## 2. Verification And Local Testing

- [x] 2.1 Add a verifier for generated `.flatpakrepo` and `.flatpakref` metadata, including URL, collection ID, app ID, suggested remote, runtime repo, and GPG key checks.
- [x] 2.2 Add a repository verifier that confirms `dev.cominotti.lushtext` is installable from the generated or staged repository and that Flatpak permissions and desktop identity match the existing LushText contract.
- [x] 2.3 Add unit-style shell tests for generator and verifier failure cases, including missing GPG key, wrong app ID, wrong collection ID, wrong runtime repo, and no-verification install instructions.
- [x] 2.4 Add Make targets for generating, verifying, and locally smoke-testing the Cominotti Flatpak repository artifacts.

## 3. Release Workflow

- [x] 3.1 Extend release dry runs to report Cominotti repository output paths, signing requirements, deploy target, and skipped deploy state without mutating public artifacts.
- [x] 3.2 Extend tag release CI to build or stage Cominotti Flatpak repository artifacts and upload them as reviewable workflow artifacts.
- [x] 3.3 Add optional deployment wiring for `flatpak.cominotti.dev` that fails or skips honestly when deploy credentials or target configuration are missing.
- [x] 3.4 Keep Flathub manifest PR generation optional and report its status separately from the Cominotti publication result.

## 4. Documentation

- [x] 4.1 Update Flatpak packaging documentation to describe the Cominotti remote as the primary Flatpak publication channel.
- [x] 4.2 Document public install commands using `lushtext.flatpakref` and manual `flatpak remote-add --from` without `--no-gpg-verify`.
- [x] 4.3 Document the expected public URL layout, GPG signing key handling, static hosting requirements, and runtime dependency on Flathub.
- [x] 4.4 Move Flathub verification and PR instructions into an optional/secondary section.

## 5. Validation

- [x] 5.1 Run the generator and verifier tests for Cominotti Flatpak artifacts.
- [x] 5.2 Run Flatpak build validation for the existing LushText manifest.
- [x] 5.3 Run release dry-run validation to confirm Cominotti publication reporting and optional Flathub reporting.
- [x] 5.4 Run `openspec validate add-cominotti-flatpak-repository --strict`.
