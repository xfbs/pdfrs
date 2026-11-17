# Building the desktop app

## Prerequisites

- Rust 1.92 or newer, installed with [rustup](https://rustup.rs)
- GTK 4.14 or newer and libadwaita 1.6 or newer, with their development files

Fedora:

```sh
sudo dnf install gcc pkgconf-pkg-config gtk4-devel libadwaita-devel
```

macOS, with [Homebrew](https://brew.sh):

```sh
brew install gtk4 libadwaita adwaita-icon-theme gettext
export GETTEXT_DIR="$(brew --prefix gettext)"
```

The app's translations go through gettext, which is part of the C library on
Linux and a library of its own on macOS. The variable tells the build where to
find it.

## Build and run

From the repository root:

```sh
cargo build --release -p specto-desktop
target/release/specto-gtk                 # start page
target/release/specto-gtk file.pdf        # open a file
target/release/specto-gtk a.pdf b.pdf     # one window per file
```

While working on it, build and start it in one step:

```sh
cargo run -p specto-desktop -- file.pdf
```

A debug build is fine for this, because the libraries that do the rendering are
optimized in debug builds too.

## Flatpak

```sh
packaging/flatpak/build.sh
flatpak run net.xfbs.specto
```

This builds the app in a sandbox and installs it for the current user. What it
needs and what it does is in
[packaging/README.md](../../packaging/README.md).

## Tests

```sh
# the shared crates (rust unit tests)
cargo test --workspace

# ui tests for the app, Linux only
apps/desktop/test-ui/run.sh python3 -I apps/desktop/test-ui/check.py
```

The second command runs the app in a headless compositor and needs a few more
packages. [TESTING.md](TESTING.md) describes it, along with the checks that
need a person.

## Translations

Text shown to people goes through the functions in `src/i18n.rs`. The comment
at the top of that file says how to write such strings. After changing any,
update the template that translations start from:

```sh
apps/desktop/po/update-pot.sh
```

The script needs GNU gettext 0.24 or newer. It also brings the translations up
to date with the template: new strings are added to them empty, and strings
that are gone are dropped.

A translation is a file in `po/` named after its language, such as `nl.po` for
Dutch. To start one, copy `po/specto.pot` to that name and fill it in, with
Poedit or any other editor for such files. Only the `.po` file belongs in the
repository. The compiled `.mo` files are built from it.

The app looks for its translations in `share/locale` next to the `bin`
directory it runs from. The Flatpak build puts them there. To try a
translation with a build from the source tree, install the translations next
to it and ask for the language:

```sh
apps/desktop/po/install.sh target/share/locale
LANGUAGE=nl cargo run -p specto-desktop
```

This needs a system language other than plain C, which has no translations.
