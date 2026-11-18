# Packaging

How to build installable packages of the desktop app, and how a release is
made.

The app ID is `net.xfbs.specto` and the name is "Specto". Both are set in
`apps/desktop/src/main.rs`, and repeated in the file names and contents in
`apps/desktop/data/` and `packaging/flatpak/`, and in
`apps/desktop/test-ui/check.py`.

## Shared files

In `apps/desktop/data/`, used by every Linux package:

- `net.xfbs.specto.desktop`: the launcher entry, which also registers the app
  for PDF files ("Open With")
- `net.xfbs.specto.metainfo.xml`: the description that software centers show,
  with the summary, links, screenshot and releases
- `icons/net.xfbs.specto.svg`: the icon
- `icons/net.xfbs.specto-symbolic.svg`: its one-color version, which GNOME uses
  where it draws small monochrome icons

Check them with:

```sh
desktop-file-validate apps/desktop/data/net.xfbs.specto.desktop
appstreamcli validate apps/desktop/data/net.xfbs.specto.metainfo.xml
```

## Flatpak

```sh
# build and install for the current user
packaging/flatpak/build.sh

# run
flatpak run net.xfbs.specto

# remove
flatpak uninstall --user net.xfbs.specto
```

The build happens in `target/flatpak`. It needs flatpak-builder, either as a
package or as the org.flatpak.Builder Flatpak, and fetches the GNOME SDK and
the Rust SDK extension from Flathub the first time.

- `flatpak/net.xfbs.specto.yml`: the manifest, on the GNOME runtime
- `flatpak/cargo-sources.py`: lists the crates from `Cargo.lock` for
  flatpak-builder to download, because the build itself has no network. The
  build script runs it each time

The app gets no access to files beyond the ones it's given. Files opened
through the file chooser, dragged in or opened with "Open With" come through
the document portal.

Saving works in the sandbox. The app writes a temporary file next to the
original and renames it into place, and the document portal allows that. File
monitors don't see changes other programs make through the portal, so the app
also looks at the file whenever its window becomes active again, and before
saving.

To check the installed Flatpak automatically, in a headless session with its
own document portal:

```sh
XDG_RUNTIME_DIR=$(mktemp -d /tmp/rt.XXXX) apps/desktop/test-ui/run.sh \
  python3 -I apps/desktop/test-ui/check-flatpak.py
```

## Releases

The desktop app and the Android app share one version, a major and a minor
number such as 0.1, in the file `VERSION` at the root. The crates have their
own version in the root `Cargo.toml`, which a release doesn't change.

A tag makes a release. The workflow in `.github/workflows/release.yml` builds
the Flatpak for x86_64 and aarch64 and the signed APK of the Android app, and
attaches them to a GitHub release with their checksums. The notes of the
release are the version's section of [CHANGELOG.md](../CHANGELOG.md), with a
link to the commits since the release before. The Android build needs the
release key as secrets of the repository
([apps/android/BUILDING.md](../apps/android/BUILDING.md)).

1. Set the version in the file `VERSION`, which both apps show.
2. In `CHANGELOG.md`, turn the heading "Unreleased" into the version and the
   date, such as "0.2 (2026-11-18)", and start a new "Unreleased" above it.
3. In `apps/desktop/data/net.xfbs.specto.metainfo.xml`, add a release entry
   with that version and the date on top of the others. Its description is a
   sentence or two for software centers, from the changelog.
4. Retake the screenshot if the window changed, with
   `apps/desktop/test-ui/readme-screenshot.sh`.
5. Commit, tag and push:

   ```sh
   git tag "v$(cat VERSION)"
   git push origin main "v$(cat VERSION)"
   ```

The workflow fails if the tag doesn't name the version in `VERSION`, or if the
changelog or the metainfo has no entry for it. "Run workflow" in the Actions tab
builds the Flatpak bundles and the signed APK of a branch without making a
release. They are kept with the run, to download and try. A bundle is
installed as the [README](../README.md#install) says.

## Publishing on Flathub

Not done yet. Flathub builds the app itself, from a tagged commit of the public
repository, in a repository of its own that holds only the manifest and the
list of crates. `flatpak/flathub.sh` writes those two files from the local
manifest.

For each release, once it is tagged and pushed as above, write the Flathub
files and check them with Flathub's linter. The screenshot's URL in the
metainfo must be reachable by then, since Flathub fetches the image from there.

```sh
tag="v$(cat VERSION)"
packaging/flatpak/flathub.sh "$tag" "$(git rev-parse "$tag^{commit}")" ../flathub-specto
flatpak run --filesystem="$(realpath ../flathub-specto)" --command=flatpak-builder-lint \
    org.flatpak.Builder manifest ../flathub-specto/net.xfbs.specto.yml
flatpak run --filesystem="$PWD" --command=flatpak-builder-lint \
    org.flatpak.Builder appstream apps/desktop/data/net.xfbs.specto.metainfo.xml
```

The first time, the two files go into a pull request against the new-pr branch
of [flathub/flathub](https://github.com/flathub/flathub). The
[submission guide](https://docs.flathub.org/docs/for-app-authors/submission)
has the steps and the review rules. Once accepted, the app gets its own
repository, flathub/net.xfbs.specto, and a release is a pull request there with
the two files written anew.

The ID says the app belongs to whoever has xfbs.net. Flathub's verification
(the tick next to the developer's name) asks for a token at
`https://xfbs.net/.well-known/org.flathub.VerifiedApps.txt`, set up from the
app's page on Flathub after it is accepted.
