# Technology decisions

The choices Specto is built on, with the alternatives that were considered.

## D1: Rendering engine

**Decision:** hayro. Rendering goes through a trait of our own, so that another
renderer can be compared with it. The other crates use hayro's parser and
interpreter directly, for the file's structure, annotations, forms and text.

hayro is pure Rust with no C dependencies, licensed MIT or Apache, and split
into a parser, an interpreter with an abstract device, and a rasterizer. The
device is also what text extraction needs. Against it: it is before 1.0 and
renders on the CPU only. PDFium would mean shipping a binary for every platform
and each Android ABI, with C++ attack surface. mupdf is licensed AGPL, which
would apply to the whole app.

**Outcome:** on 1,095 pdf.js test files, hayro had no crashes and a median of
2.8 ms per page at twice the size. It decrypts encrypted files and can render
pages without their annotations. Known gaps are knockout groups, transfer
functions, and that a render can't be cancelled.

## D2: PDF object model and writer

**Decision:** lopdf in the write crate, for the object model and saving, with
hayro for viewing.

Rendering only needs to read. Editing and saving need an object model that can
be changed, and a writer. The options were:

- **lopdf.** A mature object model in pure Rust for reading and writing, widely
  used. It is good at copying objects between documents.
- **hayro's parser with a writer of our own.** One parser for everything, but
  we would write the serializer ourselves, including cross-reference streams,
  object streams and incremental updates.
- **pdf-writer**, from the Typst project. An excellent low-level writer, but it
  has no reader. It could be combined with hayro's parser.

The concern with lopdf was that two parsers see the same file and may disagree
on malformed ones.

**Outcome:** lopdf loaded 1,092 of 1,095 corpus files and passed every editing
scenario: delete, merge, reorder, incremental annotation, encryption. Merging
needs page-import code of our own, because lopdf has none.

The concern came true in a few corpus files. lopdf drops streams without a
valid length, decrypts some streams wrongly, and loses objects behind damaged
cross-reference tables. The write crate now takes each stream's bytes from
hayro where the two disagree, and reads every result back with hayro, refusing
to save if any page differs from its source. A writer built on hayro's parser
with pdf-writer remains the fallback if lopdf's gaps grow.

## D3: Desktop UI toolkit

**Decision:** GTK 4 with libadwaita.

The desktop app targets Linux first. That takes away gpui's main advantage,
consistency across platforms, while GTK 4 already has what an app like Preview
needs: printing, list views with drag and drop, file dialogs through portals,
stylus and touchpad gestures, and accessibility. gpui is before 1.0, follows
the needs of the Zed editor, and has few widgets and no printing. We give up
idiomatic Rust, since GObject subclassing is verbose, and some rendering
headroom, though GTK's textures are composited on the GPU, which is enough for
tiles.

Because the viewer is independent of the UI, a later port to another toolkit
would only replace the desktop app.

**Outcome:** the canvas prototype measured about 55 frames per second during
sustained fast scrolling and zooming of a 384-page PDF. The app also runs on
macOS with Homebrew's GTK.

## D4: Android UI

**Decision:** Kotlin with Jetpack Compose over the Rust core, on Android 8.0
(API 26) and newer.

With Compose, the native look, the system file picker, stylus input, input
methods and accessibility work without extra effort, and Rust only renders and
holds the logic. Slint, egui and Dioxus would keep one language but are weaker
on all of those. Android's built-in PDF renderer can't edit.

**Outcome:** the app is built this way. Tiles are written straight into Android
bitmaps from Rust.

## D5: FFI for Android

**Decision:** UniFFI for the coarse API, and hand-written JNI for what runs on
every frame.

UniFFI generates the Kotlin bindings for everything called occasionally:
opening documents, changing modes, callbacks. Anything per frame or per tile
goes through a few hand-written JNI functions instead. The reasons are
measurements:

- **Tile pixels.** Returning a 6.6 MB page through UniFFI cost about 6 ms
  extra. JNI copies once into the locked bitmap.
- **Calls per frame**, such as scrolling, zooming and planning a frame. Even a
  trivial UniFFI call cost 200 to 370 microseconds, against about 4
  microseconds of actual work. On a Pixel 9a, moving these to JNI cut slow
  frames from 1.65% to 0.61%.

JNI functions take a session id, not a raw pointer, and frames are written into
a byte buffer in a documented layout. The JNI code only compiles for Android,
so CI lints the FFI crate a second time for an Android target.

## D6: License

**Decision:** MIT or Apache 2.0, at the user's option.

This is the convention in the Rust ecosystem, and matches hayro and lopdf.
Apache 2.0 adds an explicit patent grant, and MIT keeps us compatible with
GPLv2 projects. We avoid mupdf and other copyleft dependencies.

## D8: Annotations are saved as annotations

**Decision:** markup is written as standard PDF annotation objects with
generated appearance streams, as Preview does.

Other viewers can then show, edit and delete it. Flattening into page content
isn't the default. At most it becomes a separate export option later.

## D9: Encrypted PDFs follow Preview, except for permission flags

**Decision:** match what Preview does with passwords. Permission flags are
shown and not enforced.

Preview's behavior, checked on a Mac:

- **Open password.** Preview asks for it on opening. Saving keeps the document
  encrypted with the same password.
- **Setting passwords.** Preview manages both the open password and the owner
  (permissions) password in a panel reached from the File menu, where each can
  be added, changed or removed. This is a setting of the document that is saved
  with the file, not an option of exporting.
- **Owner password only**, where the file opens without asking but editing or
  printing is restricted. This is not checked yet. We assume that viewing
  works, and that restricted actions ask for the owner password first.

What this means for Specto:

- A dialog on the document for the two passwords. Its changes are saved like
  any other edit.
- We need to write encrypted PDFs, not only read them. Saving again keeps the
  original algorithm, and new encryption uses AES-256. lopdf handles all of
  this, and hayro decrypts for viewing.
- Permission flags are not enforced, and no setting turns enforcing on. A file
  with only an owner password opens without one, so every reader already holds
  the key to its content and the flags are a request to the software. Evince
  and MuPDF ignore them by default too. A setting to enforce them would have no
  one to turn it on, and enforcing by default would have people look for the
  way to turn it off.
- What the file asks for stays visible and stays in the file. Properties lists
  what is not allowed, and saving keeps the encryption with its flags as they
  are.

**Outcome:** opening with a password, saving with the encryption and flags
kept, and showing the flags are implemented. The password dialog is not.

## D10: Other document formats

**Decision:** images on the desktop later, and PDF only on Android.

Preview and GNOME Papers open more than PDF. We stay with PDF until the PDF
features work well, then aim for parity with Preview on the desktop, which in
practice means PDF and images.

- **Images** (PNG, JPEG, TIFF, WebP) are cheap: an image is a one-page
  document. They come to the desktop after the PDF milestones, and would also
  allow inserting an image as a page.
- **Comic archives** are cheap too, being a zip of images. Maybe, once images
  exist.
- **DjVu, XPS and PostScript** are expensive. Each needs its own engine, and
  the mature DjVu library is GPL, which conflicts with D6. Out of scope.
- **EPUB** is very expensive. It reflows, so it doesn't fit the model of pages
  and tiles. Out of scope.

On Android the system already handles images, and the gap is a good PDF viewer.
Each backend other than PDF would be a Cargo feature that the Android build
leaves off, so it adds no size there.

Editing stays specific to PDF. Marking up images the way Preview does is a
separate feature and not part of this decision.

What images would need in the code:

- The renderer, the scheduler and the viewer only know the document trait,
  which an image can implement, with its size as the page.
- The session reads the outline, the text and the forms from the PDF directly.
  Those would have to become things a document may or may not have.

## Open questions

- Do signed PDFs need special handling beyond saving incrementally so that
  signatures stay valid? Preview warns that edits invalidate signatures.
