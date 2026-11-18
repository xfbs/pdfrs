# Specto for Android

A fast, pleasant PDF reader for Android phones and tablets. It is built with
Rust and Jetpack Compose.

<p align="center">
  <img src="screenshot-phone.png" width="20%" alt="Specto on a phone: a page of a manual with diagrams, the page number, and a strip of page thumbnails along the bottom">
  &nbsp;
  <img src="screenshot-tablet.png" width="72%" alt="Specto on a tablet: two pages side by side, with the table of contents in a sidebar">
</p>

Android has a few open-source PDF viewers, but I haven't found one that I love.
They either don't feel finished or are missing features I expect. Many can
render a page crisply and turn it smoothly. What I miss is the combination: a
polished interface, search, full screen, two pages side by side on a tablet,
and light editing. Specto is my attempt at a reader that has all of it.

## Features

The main features are below, and the full list is in
[FEATURES.md](FEATURES.md).

- **Reading**: one page at a time with swipes, each page fitted to the screen,
  or continuous scrolling
- **Zoom**: pinch and double tap, fit page or fit width, with pages sharp at
  any zoom
- **Navigation**: a thumbnail strip, a grid of all pages, the table of
  contents, links, and printed page numbers ("iii (3 / 240)")
- **Search**: ignoring case and accents, with all matches highlighted
- **Text**: selecting, copying and sharing
- **Markup**: highlighting, underlining and striking through text, saved as
  annotations that other PDF programs show too
- **Forms**: filling in text fields, checkboxes, radio buttons, menus and
  lists
- **Editing**: rotating, deleting and reordering pages, with undo and redo, and
  saving the result. Unsaved edits are kept until they are saved or discarded
- **Tablets**: a sidebar, and two pages side by side on wide windows,
  optionally with the cover page on its own
- **Printing and sharing**: with the system's print dialog and share sheet
- **Encryption**: password-protected PDFs
- **History**: recent documents, and continuing where you left off
- **Usability**: full screen, keyboard navigation, light and dark mode, document
  properties, and optionally keeping the screen on and turning pages with the
  volume keys
- **Languages**: more than 30, following the device's language

Coming next: more kinds of annotations, and colors to mark text with.

## Implementation

The core is written in Rust and shared with the desktop app (see
[crates/](../../crates/README.md)). It is compiled into one native library,
which Kotlin reaches through UniFFI, plus a few hand-written JNI functions for
the work done on every frame, where UniFFI's overhead was too high.

The app itself is written in Kotlin with Jetpack Compose. It draws the rendered
tiles and handles gestures, the file picker and recent documents. More detail
is in [docs/ARCHITECTURE.md](../../docs/ARCHITECTURE.md).

## Building

Build it and install it on a connected device or emulator:

```sh
cd apps/android
./gradlew installDebug
```

What to install first, release builds and their signing key, and emulators on
Intel machines are in [BUILDING.md](BUILDING.md). The app runs on Android 8.0
(API 26) and newer.

## Testing

```sh
cd apps/android

# unit tests
./gradlew testDebugUnitTest

# UI tests on an emulator
ANDROID_SERIAL=emulator-5554 ./gradlew connectedDebugAndroidTest
```

The UI tests cover most of [FEATURES.md](FEATURES.md) on phone-sized and
tablet-sized screens. Set the serial when a phone is plugged in as well,
because a test run installs and then removes the app on its target.

CI builds the app, including the release build, and runs the unit tests on
every change. It runs the UI tests on an emulator for pushes to the main
branch. What tests can't judge (how it looks and feels, the file picker, pinch
zoom) is checked by hand, as described in [TESTING.md](TESTING.md).

## Project structure

The layout is flatter than Android's usual one, which would put the code under
`app/src/main/kotlin/net/xfbs/specto`. The Gradle project is the app, and
`build.gradle.kts` says where its parts are. Sorry to developers who are used
to that deeply nested structure, but it gives me the ick.

The code is in `src/`, mostly one file per part of the screen. A few things
that aren't obvious from the file names:

- `src/Native.kt`: the hand-written JNI functions. The rest of the Rust API
  comes through the bindings that UniFFI generates during the build
- `src/ViewFrame.kt`: one frame of the view as planned in Rust, decoded from
  the JNI call
- `src/DocumentStore.kt`: recent documents and where each was left off
- `res/values/strings.xml`: everything the app says, in English. The
  translations in `res/values-<language>/` come from Crowdin. A unit test
  checks that they have no texts of their own and keep the placeholders
- `proguard-rules.pro`: what code shrinking must leave alone in a release build
- `test/`, `test-ui/`: the unit tests and the UI tests. The test PDFs in
  `test-ui/assets` are written by `test-ui/make-fixtures.py`
- `screenshot-phone.png`, `screenshot-tablet.png`: the screenshots above, taken
  on the emulators with the same document as the desktop app's screenshot
  (hwloc's manual, BSD license)
