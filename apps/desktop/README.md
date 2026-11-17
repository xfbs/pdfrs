# Specto for the desktop

A fast, responsive PDF viewer with light editing capabilities. It is built with
Rust and GTK 4 and designed for the GNOME desktop on Linux. It also runs on
macOS.

<p align="center">
  <img src="screenshot.png" width="50%" alt="Specto showing two pages side by side, with the table of contents in the sidebar">
</p>

There are several PDF readers for Linux, but few support basic editing, such as
merging PDFs, rotating pages, adding annotations or filling in forms. The
editors that do support these are often not pleasant to use. Specto is meant to
fill that gap. It is not a full PDF editor, but it does the common things
quickly.

## Features

The main features are below, and the full list is in
[FEATURES.md](FEATURES.md).

- **Rendering**: sharp at any zoom, including on HiDPI and fractionally scaled
  screens, and responsive on huge or very complex pages
- **Layouts**: single page, continuous and two-page, optionally with the cover
  page on its own
- **Zoom**: fit width, fit page and free zoom, with touchpad pinch and
  Ctrl+scroll
- **Navigation**: thumbnails, the table of contents, links, and printed page
  numbers ("iv", "A-3")
- **Text**: selecting and copying by dragging, double-click for a word,
  triple-click for a line
- **Search**: ignoring case and accents, with all matches highlighted
- **Editing**: rotating, deleting and reordering pages, inserting pages from
  other PDFs, and exporting a selection, with undo and redo
- **Markup**: highlighting, underlining and striking through text, freehand
  drawing, shapes, text boxes, notes and signatures, saved as standard
  annotations
- **Forms**: ticking checkboxes, choosing radio buttons, typing into text
  fields, and choosing from menus and lists, saved as standard form values
- **Printing**: with the system's print dialog
- **Encryption**: opens password-protected PDFs, and keeps them protected when
  saving
- **History**: recent files, and remembering where you were in each file

Coming next: the remaining kinds of form fields, such as lists that take
several choices.

## Implementation

The core is written in Rust and shared with the Android app (see
[crates/](../../crates/README.md)). The desktop app is a thin layer on top: it
draws the rendered tiles, handles input, and provides the windows, menus and
sidebar with GTK 4 and libadwaita. More detail is in
[docs/ARCHITECTURE.md](../../docs/ARCHITECTURE.md).

## Building

Build and run it with Cargo, from the repository root:

```sh
cargo build --release -p specto-desktop
target/release/specto-gtk file.pdf
```

What to install first on Fedora and macOS, and how to build the Flatpak, is in
[BUILDING.md](BUILDING.md).

## Testing

The automated tests are in `test-ui/check.py`, which runs the app in a headless
compositor and checks opening, navigation, layouts, zoom, saved state
and more, at several screen scales. It runs in CI.

For manual testing, [TESTING.md](TESTING.md) has the setup, test files and the
checks that need a person, on Linux and macOS.

## Project structure

The code is in `src/`, mostly one file per part of the window. A few things
that aren't obvious from the file names:

- `src/canvas.rs`: the page view, a scrollable widget that draws the frames
  planned by the shared viewer
- `src/document.rs`: an open document, which is the shared session plus what
  belongs to GTK (the cache of tile textures)
- `src/debug.rs`: test hooks, only enabled with `SPECTO_DEBUG=1`
- `po/`: the translations, the template they start from, and the scripts that
  write the template and install the translations
- `test-ui/`: the automated checks and the script that runs them in a headless
  compositor
- `screenshot.png`: the screenshot above, made by
  `test-ui/readme-screenshot.sh`
