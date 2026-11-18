# Task list

Milestones are ordered by dependency. Within each milestone, finished tasks
come first, in the order they were done, followed by the open ones. Finished
tasks are trimmed now and then, down to those that add context, such as a
measurement or what an open task builds on.

What the apps do today is described in
[apps/desktop/FEATURES.md](../apps/desktop/FEATURES.md) and
[apps/android/FEATURES.md](../apps/android/FEATURES.md). References like D9
point to [DECISIONS.md](DECISIONS.md).

## M0: Evaluations and foundations

- [x] hayro evaluation: rendering the corpus, compared with Poppler, with
  failures and timing recorded (D1)
- [x] lopdf evaluation: delete, merge, reorder, an incremental highlight, and
  encrypted saves (D2, D9). The code was in `spikes/lopdf-spike` until commit
  7273d64
- [x] Preview's handling of passwords checked on a Mac (D9)
- [x] GTK 4 canvas evaluation, also on real hardware: a custom widget drawing
  tiles, scrolling and zooming at about 55 frames per second (D3)
- [x] Android evaluation: a Compose app in which Rust renders a page into a
  bitmap, through cargo-ndk and UniFFI (D4, D5)
- [ ] More of a test corpus: the PDFium test files, and fixtures of our own for
  forms, rotated pages, crop boxes that differ from the media box, huge pages,
  scans and encrypted files.
- [ ] PDFium reference renders for the regression run. Poppler turned out to be
  wrong in about half of the worst cases.
- [ ] Check how Preview handles files with only an owner password, where
  nothing is asked on opening but editing or printing is restricted, and update
  D9.

## M1: Core viewer

- [x] Guards for expensive pages: they render one tile at a time, workers free
  their decoded images after a heavy page, and extra workers start while
  renders are stalled
- [x] hayro's render cache kept between renders, which made tiles 1.4 to 6
  times faster
- [x] Large files (64 MB and up) memory-mapped instead of read
- [x] HiDPI: pages, tiles and the canvas placed on whole device pixels, zoom
  levels that are only ever scaled down, and settled views drawn from tiles at
  the exact device scale
- [x] HiDPI: tiles drawn as plain texture nodes, which fixed the blur
- [x] Back and forward after a jump, in the viewer and on the desktop
- [x] Trim margins in the viewer and on the desktop
- [ ] Upstream in hayro: decode images only for the region and resolution
  being drawn. One tile of a 212-megapixel JPEG 2000 page still costs 4.5 GB
  and 10 seconds at 100%, which no scheduling can fix.
- [ ] Confirm the HiDPI sharpness by eye, on the Mac at scale 2 and on a real
  Linux display with fractional scaling.
- [ ] The fonts of a document in the Properties dialog. They need a walk over
  every page's resources in the background.
- [ ] Bookmarks set by the reader, listed with the contents. Decide where they
  live. Kept by the app per document, there is nothing to save, but they stay
  on the device. Written as outline entries, they travel with the file, but
  need a save. Kept by the app, they also need a way to recognize a document
  that survives saving it, and so will the reading positions.

## M2: Text

- [x] hayro evaluation: its mapping from glyphs to Unicode is good enough to
  build on
- [x] The text layer: glyphs with their positions, words and lines in content
  order, and right-to-left text in logical order
- [ ] Reading order by layout (columns, blocks), for files whose content stream
  isn't in reading order. That is about 5% of the corpus, mostly forms and
  flyers.
- [ ] Fallbacks for glyphs without a Unicode mapping: a reverse lookup in the
  embedded TrueType font's character map, and the character code read in the
  font's encoding for Type 3 fonts.
- [ ] Investigate and report upstream: hayro ignores the existing Unicode
  tables in `complex_ttf_font.pdf` and `issue16538.pdf`.
- [ ] Search follow-ups: right-to-left text in visual order, matches spanning
  pages, and options for whole words and matching case.

## M3: Page editing

- [x] Saving as a full rewrite, with the result read back and checked page by
  page. On the corpus, 1,066 of 1,077 files come out identical, 11 are refused
  and none is damaged
- [x] Desktop: noticing that the open file changed on disk
- [x] Android: unsaved edits kept as a draft in the shared store
- [ ] Save the 11 corpus files that are refused. lopdf can't read damaged
  cross-reference tables or page trees that hayro recovers, and one file has an
  encryption dictionary that lopdf encrypts wrongly. Building the objects from
  hayro's reading of the file would be one way.
- [ ] Merge resources shared between sources, such as fonts embedded in both of
  two merged files.
- [ ] Android: notice a changed file when the app resumes, and reload or ask,
  like the desktop.
- [ ] Desktop: unsaved edits kept as a draft with the shared store, as on
  Android, and the saved signatures moved into its images.
- [ ] Draft follow-ups: the undo history in drafts, and a mark on the start
  screen for documents with unsaved edits.
- [ ] A memory-mapped file that another program truncates in place crashes the
  app when the missing part is read. Programs that save by renaming, as most
  do, are safe. See the task on mapped files that shrink in M7.

## M4: Markup

- [x] Desktop: annotations a file already has can be picked and deleted. The
  page is then rendered from a copy without the annotation
- [x] Saving checks that a changed or deleted annotation is where it was found
- [x] Markup written by Specto is recognized when a file is opened again, so it
  can still be changed
- [x] Desktop: signatures, drawn or taken from a photo, kept with the app's
  data
- [x] Desktop: text boxes in the three font families no PDF needs to embed
- [x] Saves that only add markup or form values are appended to the file,
  also for encrypted files
- [ ] Page copies for deleted annotations are written while the user waits.
  That is fine for ordinary files. For very large ones, write them in the
  background, and keep lopdf's reading of a file between two deletions.
- [ ] Ask upstream for a way to leave out single annotations in hayro, which
  would make the page copies unnecessary.
- [ ] Markup toolbar follow-ups: line width and opacity, a fill color for
  shapes, a custom color, remembering the colors, the text style and the last
  tool between runs, and a layout for narrow windows, where the bar scrolls
  sideways for now.
- [ ] Drawing follow-ups: several strokes as one annotation, stylus pressure,
  arrowheads for lines, and Shift for squares, circles and straight angles.
- [ ] Check by hand how highlights look on a real screen and on the Mac, and
  that Preview, Papers and a browser show and can delete what was saved.
- [ ] Squiggly underlines drawn as a wavy line.
- [ ] Text follow-ups: alignment, typing on pages shown rotated, and any font
  of the system together with characters beyond Windows Latin. Both of those
  need a font embedded in the file and are one piece of work. Such characters
  are saved as question marks now.
- [ ] A text box being dragged shows only its frame until it is dropped. Draw
  its text in the overlay during the drag.
- [ ] Adjusting follow-ups: handles on the sides, Shift to keep proportions,
  arrow keys to nudge, snapping, and moving several at once.
- [ ] Markup list follow-ups: filling it in the background for very long
  documents, the author and date, filtering by kind, and replies.
- [ ] Signatures from a photo: the paper isn't taken away cleanly enough yet.
  Find out on real photos what is left over or lost, and improve it.
- [ ] Signature follow-ups: a stylus or touchscreen for drawing, naming and
  reordering saved signatures, cropping a picture by hand, placing any image as
  a stamp, and encrypting the saved signatures.
- [ ] Android: draw a signature by finger or stylus, and a way to bring it to
  the desktop.

## M5: Forms

- [x] Filled-in values in the edit document, with undo, and saved with new looks
- [x] Values a file leaves to the reader to draw are shown
- [x] Every kind of field filled in, in both apps
- [ ] An embedded fallback font for text outside Latin.
- [ ] Android: typing into a text field on the page instead of in a dialog,
  and a way from one field to the next from the keyboard.
- [ ] Forms of inserted files. Only the form of the opened file is kept.
- [ ] An optional flattened export of a filled form.

## M6: Android

- [x] The session crate: one type for the open document, shared by both apps
- [x] Tiles: Kotlin owns a bitmap per tile and fills it through JNI, with the
  cache bookkeeping in Rust
- [x] The work done on every frame moved from UniFFI to JNI
- [x] UI tests on a phone and a tablet emulator, in CI for pushes to the main
  branch
- [x] Turning the page returns to the fitted view
- [x] Opening through a file descriptor, without reading the file into the
  app's memory
- [ ] Pages of other files through the FFI, when the app gets a way to ask for
  them.
- [ ] Find out what Preview does with the zoom when turning a page while zoomed
  in, on iOS and on macOS in the single-page layout. Then decide whether we
  keep returning to the fitted view or carry the zoom over to the next page.
- [ ] A rubber band at the first and last page.
- [ ] The strip causes an occasional slow frame when it loads new thumbnails
  after a page turn (30 ms at the 99th percentile, against 12 ms in full
  screen).
- [ ] The page number in the top bar on tablets, as on the desktop.
- [ ] Page organizer follow-ups: thumbnails that make room while pages are
  dragged over them, Undo offered right after a delete, and dragging pages out
  to another app or in from one.
- [ ] Markup follow-ups: colors, and the other kinds of annotation, with a
  toolbar like the desktop's.
- [ ] Markup tools with stylus support: pressure, palm rejection, and scrolling
  with a finger while the pen draws.
- [ ] Forms, with a native text field overlay and input methods.
- [ ] F-Droid: check that the Gradle dependencies are all free software, make
  the build reproducible, and write the metadata.
- [ ] Back after a jump: the system's Back returns to where a link or outline
  entry was followed from. The shared code keeps the places and has the calls
  for it.
- [ ] Trim margins: a switch in the view menu for the paged view mode,
  remembered per document. The FFI has the calls.
- [ ] Bookmarks in the Pages and Contents view. Depends on the task in M1.
- [ ] Document properties follow-ups: how the file is protected, and its fonts.

## M7: Polish and hardening

- [x] Printing on the desktop, with pages rendered at 300 dpi
- [x] Android: code shrinking in the release build, which took the APK from
  16.4 to 9.4 MB
- [x] The app's name and ID settled: Specto, `net.xfbs.specto`
- [x] Permission flags: shown in Properties, kept on saving, not enforced (D9)
- [x] A tag builds both apps and publishes them as a GitHub release, with the
  changelog as its notes
- [ ] Vector printing on the desktop: send the pages themselves instead of
  images, for smaller files when printing to a file and for selectable text.
- [ ] Document security dialog, like Preview's: add, change or remove the two
  passwords, saved with the document. The lopdf evaluation shows how to encrypt
  a file from scratch (`spikes/lopdf-spike` at commit 7273d64).
- [ ] Fuzzing of the parser and the save path.
- [ ] Performance pass: profile big and complex documents, startup time and
  memory.
- [ ] Accessibility: operation by keyboard alone, and labels for screen
  readers.
- [ ] Dark mode for page content, as an option.
- [ ] Publish on Flathub (steps in
  [packaging/README.md](../packaging/README.md)). The metainfo, the script that
  writes the Flathub files and tagged releases are ready. Left to do: the pull
  request to Flathub.
- [ ] deb and rpm packages. They need libadwaita 1.6 or newer from the
  distribution.
- [ ] macOS: check where About Specto shows up. It is under Help in our menu
  bar. GTK may also put one into the application menu, and then one of the two
  should go.
- [ ] macOS app bundle: file association, Dock icon, bundled GTK.
- [ ] Performance regression checks in CI: tile render times, frame times and
  peak memory on a few fixed documents, compared against stored numbers with a
  tolerance. The canvas evaluation had a benchmark mode to start from, in
  `spikes/gtk-canvas` at commit f32a26a.
- [ ] Preferences: the default layout and zoom for documents opened for the
  first time.
- [ ] Decide whether page content is exposed to screen readers (the text
  layer, reading order, tagged PDF structure), and if so, plan it.
- [ ] Desktop: a crash log that the user can attach to a bug report.
- [ ] Android: include native crashes that aren't panics, such as running out
  of memory, in the crash log. Android 11 and later keep a record of those.
- [ ] Mapped files that shrink. A file of 64 MB or more is memory-mapped, and
  if another program truncates it in place while it is open, reading the
  missing part crashes the app. On Android a sync or download app could do
  that. Decide whether and how to guard against it. Ideas:
  - Notice the change first and stop reading, through the file monitor or by
    checking size and modification time before each render.
  - Map only files nobody else can write. On Android that means copying large
    files from other apps into the app's cache first, at the cost of the space.
  - A signal handler that turns the fault into a failed render.
  - Reading by position instead of mapping, which hayro's input doesn't allow
    today.
- [ ] Touch on Linux, for convertible laptops and phones: drag to scroll, long
  press to select, a layout for narrow windows, and touch input in the headless
  checks.

## M8: Internationalization

Each app uses its platform's own mechanism, and the core crates carry no text
that users see.

- [x] Translations are contributed on Crowdin
- [x] More than 30 languages for both apps, most of them machine translated
- [x] Checks that catch broken translations: a unit test of the Android
  placeholders, and msgfmt on the desktop catalogs. Crowdin's own checks
  missed a lost number in plurals
- [ ] Core: no text for users from the crates. The frontends map error kinds to
  their own translated messages instead of showing the crates' English text.
- [ ] Numbers through the locale: page counts and the zoom percentage.
- [ ] Desktop: translations on macOS. Check the build with Homebrew's gettext,
  which nobody has tried yet, and install the catalogs there.
- [ ] The machine translations checked by people who speak the language. German
  and Dutch are checked already.
- [ ] Check on a device that Android finds the translations whose language
  codes differ from Crowdin's: Norwegian (`no`, where devices report `nb`),
  Hebrew (`he`, which older Android versions call `iw`) and Chinese (`zh`,
  shown to readers of both simplified and traditional characters). Map the
  codes in `crowdin.yml` where they don't match.
- [ ] Right-to-left UI languages: check both apps under such a locale. GTK and
  Compose mirror on their own, but the sidebar side, the thumbnail strip and
  icons with a direction need a look. The page canvas does not mirror.
- [ ] Binding direction: one property in the viewer for both the order of pages
  in a two-page spread and the swipe direction. It defaults to what the
  document states, independent of the UI language, with a toggle per document,
  since many right-to-left PDFs don't state it.
