<p align="center">
  <img src="apps/desktop/data/icons/net.xfbs.specto.svg" width="128" height="128" alt="">
</p>

<h1 align="center">Specto</h1>

<p align="center">
  A fast PDF viewer and light editor for Linux and Android, written in Rust.
</p>

<p align="center">
  <a href="apps/desktop/README.md">Desktop</a> ·
  <a href="apps/android/README.md">Android</a> ·
  <a href="docs/ARCHITECTURE.md">Architecture</a> ·
  <a href="docs/TASKS.md">Plan</a> ·
  <a href="#translations">Help translate</a>
</p>

<p align="center">
  <img src="apps/desktop/screenshot.png" width="60%" alt="Specto on the desktop: two pages side by side, with the table of contents in the sidebar">
  &nbsp;
  <img src="apps/android/screenshot-phone.png" width="21%" alt="Specto on a phone: a page of a manual, the page number, and a strip of page thumbnails along the bottom">
</p>

## Why?

I enjoy working with Preview on macOS. When I want to read a PDF, or do some
light editing (reorder or delete pages, highlight something, fill in a form),
it's what I reach for: it's fast, it stays out of the way, and it does the
common things well. On Linux I haven't found anything equivalent, so this is my
attempt at one.

I also miss a decent open-source PDF viewer for Android. One that is pleasant
to use, asks for no permissions, has no ads and never goes online, and does the
common things well, such as full screen, different zoom and scroll modes, and
filling in forms.

Since the hard parts (rendering, layout, zooming, text) are the same
everywhere, they live in a shared Rust core, and each app is a thin native
layer on top of it.

## Goals

Rough feature parity with Preview, not Adobe Acrobat:

- **Viewing**: sharp rendering on HiDPI screens, zoom, single page, continuous
  and two pages side by side, thumbnails, table of contents, search, text
  selection
- **Page editing**: delete, reorder and rotate pages, insert pages from another
  PDF, merge documents
- **Markup**: highlight, underline and strike through text, freehand drawing,
  shapes, text boxes, sticky notes
- **Forms**: fill in form fields (text, checkboxes, radio buttons, dropdowns
  and lists) and save the filled values
- **Censor**: remove information from documents safely

Not planned, at least for now: editing existing page content (reflowing text),
XFA forms, creating digital signatures, JavaScript, OCR, PDF/A conversion.

## Install

Builds are on the [releases page](https://github.com/xfbs/specto/releases).
There are builds for Android (the APK, for phones and tablets with a 64-bit ARM
processor) and Linux (a Flatpak bundle for x86_64 and one for aarch64).

The Flatpak bundle is installed from a terminal:

```sh
flatpak install specto-0.1-x86_64.flatpak
```

Neither updates itself, so a newer release is installed over the old one the
same way. I plan to bring Specto to F-Droid and Flathub once it has had more
testing.

## Status

Both apps are usable as PDF viewers. They render pages sharply at any zoom and
on HiDPI screens, and stay responsive on huge documents. They have the usual
layouts, thumbnails, the table of contents, links and printed page numbers,
open encrypted files, and remember where you were in each document. In both you
can select, copy and search text, highlight, underline and strike through it,
print, edit pages by rotating, deleting and reordering them, and fill in forms:
checkboxes, radio buttons, text fields, menus and lists.

The desktop app can also insert pages from other PDFs and export a selection of
pages. It has more markup too: freehand drawing, shapes, text boxes, notes and
signatures.

The detailed plan is in [Tasks](docs/TASKS.md).

## Translations

Specto is translated into more than 30 languages. Most of the translations
were made by machine and haven't been checked by a native speaker yet, so
reviewing them is the most useful help there is, and it takes no programming.

Translations are made on [Crowdin](https://crowdin.com/project/specto). You can
sign in, pick your language, suggest a better translation where one is wrong,
and vote for the ones that are right. A note beside many texts says where in
the app it appears. Changes to the translated files in this repository would be
overwritten, so they are made on Crowdin as well.

If your language is missing, or you would rather report a mistake than fix it,
[open an issue](https://github.com/xfbs/specto/issues).

## Building

Each app has its own instructions, with what to install first:

- [Building the desktop app](apps/desktop/BUILDING.md): Fedora and macOS, and
  the Flatpak
- [Building the Android app](apps/android/BUILDING.md): phones and emulators

The shared crates and the tools need only Rust 1.92 or newer:

```sh
cargo build --release
cargo test --workspace
```

The tests check saved files with qpdf where it is installed.

## Layout

- [`crates/`](crates/README.md): the shared Rust core, which does the
  rendering, page layout, text, editing and saving
- [`apps/desktop`](apps/desktop/README.md): the desktop app, in Rust with GTK 4
  and libadwaita
- [`apps/android`](apps/android/README.md): the Android app, in Kotlin with
  Jetpack Compose
- [`tools/`](tools/README.md): development tools, such as the specto
  command-line tool
- [`packaging/`](packaging/README.md): installable packages of the desktop app
  (Flatpak)
- [`corpus/`](corpus/README.md): fetches the test corpus
- `docs/`: project-wide documentation

## Documentation

- [Architecture](docs/ARCHITECTURE.md): data model, rendering
  pipeline, how the apps plug in
- [Decisions](docs/DECISIONS.md): technology choices, the alternatives
  considered, and open questions
- [Tasks](docs/TASKS.md): the task list, by milestone
- [Specto Desktop Features](apps/desktop/FEATURES.md) and
  [Specto Android Features](apps/android/FEATURES.md): what each app does
- [Specto Desktop Testing](apps/desktop/TESTING.md) and
  [Specto Android Testing](apps/android/TESTING.md): testing each app, by hand
  and automated

## Similar projects

- [Censor](https://codeberg.org/censor/Censor): Linux app to censor PDFs,
  inspiration for adding that feature to Specto
- [GNOME Papers/Document Viewer](https://apps.gnome.org/Papers/): GNOME PDF
  reader app, inspiration for Specto
- [Evince](https://gitlab.gnome.org/GNOME/evince): PDF reader for GNOME
- [Apple Preview](https://en.wikipedia.org/wiki/Preview_(Apple)): Apple's PDF
  reader app, inspiration for Specto

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT
license](LICENSE-MIT), at your option.
