# Task list

Milestones are ordered by dependency. Within each milestone, finished tasks
come first, in the order they were done, followed by the open ones.

What the apps do today is described in
[apps/desktop/FEATURES.md](../apps/desktop/FEATURES.md) and
[apps/android/FEATURES.md](../apps/android/FEATURES.md). References like D9
point to [DECISIONS.md](DECISIONS.md).

## M0: Evaluations and foundations

- [x] Cargo workspace
- [x] CI on GitHub Actions, in Fedora containers: formatting, lints and tests
  for the Rust code, and the headless checks of the desktop app
- [x] License decision: MIT or Apache 2.0 (D6)
- [x] Test corpus: the pdf.js test PDFs
- [x] hayro evaluation: rendering the corpus, compared with Poppler, with
  failures and timing recorded (D1)
- [x] hayro evaluation: rendering page content without annotations
- [x] lopdf evaluation: delete, merge, reorder, and an incremental highlight
  (D2)
- [x] lopdf evaluation: encrypting, encrypted incremental save, and editing an
  encrypted file with its encryption kept (D9)
- [x] Preview's handling of passwords checked on a Mac (D9)
- [x] GTK 4 canvas evaluation: a custom widget drawing tiles, with continuous
  scrolling and zooming at about 55 frames per second (D3)
- [x] Canvas evaluation checked on real hardware
- [x] Android evaluation: a Compose app in which Rust renders a page into a
  bitmap, through cargo-ndk and UniFFI (D4, D5)
- [x] The lopdf evaluation code removed. It was in `spikes/lopdf-spike`, last
  changed in commit 7273d64
- [ ] GitLab CI, if the repository moves there.
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
- [x] A spinner over expensive pages that aren't sharp yet, on the desktop and
  on Android
- [x] hayro's render cache kept between renders, which made tiles 1.4 to 6
  times faster
- [x] Typed coordinate spaces and page geometry
- [x] Opening and decryption, metadata, the outline, links and named
  destinations, with every walk through a file bounded
- [x] Large files (64 MB and up) memory-mapped instead of read
- [x] Page labels, in all five numbering styles, shown and accepted by both
  apps
- [x] The document trait with its hayro backend, rendering tiles at any scale
- [x] The render scheduler and the tile cache
- [x] HiDPI: pages and tiles placed on whole device pixels, and zoom levels
  that are only ever scaled down
- [x] HiDPI: tiles drawn as plain texture nodes, which fixed the blur
- [x] HiDPI: settled views drawn from tiles at the exact device scale
- [x] HiDPI: the canvas snapped to a whole device pixel at fractional scales
- [x] Tile cache and scheduler fixes found while reviewing the Android app
- [x] Viewer: the four layouts, and the cover page alone
- [x] Viewer: viewport, zoom anchored at the pointer, scrolling, paging,
  hit-testing, and frames with fallback tiles and prioritized requests
- [x] Desktop: the canvas widget, with native scrollbars, kinetic scrolling,
  and zoom by scrolling and pinching
- [x] Desktop: thumbnail sidebar, page entry, layout and zoom menus, keyboard
  navigation
- [x] Desktop: outline sidebar
- [x] Desktop: opening by dialog, drag and drop or command line, the password
  prompt and the start page
- [x] Desktop: recent files, and the view state remembered per file
- [x] Desktop: one scheduler shared by all windows
- [x] Desktop: headless checks in CI, at scales 1, 1.25 and 2
- [x] Desktop: headless checks with a virtual pointer and keyboard
- [x] Desktop: links
- [x] Android: outline and links
- [x] Desktop: Properties dialog, with how the file is protected
- [x] Back and forward after a jump, in the viewer and on the desktop
- [ ] Upstream in hayro: decode images only for the region and resolution
  being drawn. One tile of a 212-megapixel JPEG 2000 page still costs 4.5 GB
  and 10 seconds at 100%, which no scheduling can fix.
- [ ] Confirm the HiDPI sharpness by eye, on the Mac at scale 2 and on a real
  Linux display with fractional scaling.
- [ ] Trim margins: a view option that lays pages out by the box of what is
  drawn on them, not by the crop box. It matters most on a phone, where white
  margins take up to a third of the width. It only crops the view. Open
  questions:
  - Where the box comes from. The text layer's positions are cheap but miss
    figures, and scans entirely. Candidates are the bounds of everything drawn,
    collected the way the text layer collects glyphs, or the pixels of the
    placeholder tile that aren't background.
  - How to keep it steady from page to page. One box per document, or one each
    for left and right pages, would be steadier than one per page.
- [ ] The fonts of a document in the Properties dialog. They need a walk over
  every page's resources in the background.
- [ ] Bookmarks set by the reader, listed with the contents. Decide where they
  live. Kept by the app per document, there is nothing to save, but they stay
  on the device. Written as outline entries, they travel with the file, but
  need a save. Kept by the app, they also need a way to recognize a document
  that survives saving it, and so will the reading positions.
- [ ] Link preview on the desktop: hovering over an in-document link shows the
  area around its target, for footnotes and citations in papers. It works where
  destinations carry a position, as LaTeX's do. It comes after back and
  forward, which serve the same need.

## M2: Text

- [x] hayro evaluation: its mapping from glyphs to Unicode is good enough to
  build on
- [x] The text layer: glyphs with their positions, words and lines in content
  order, and right-to-left text in logical order
- [x] Text selection by dragging, double click and triple click, and copying
- [x] Search on the desktop, ignoring case and accents, with pages indexed in
  the background
- [x] A cache of the text layer per page
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

- [x] The edit model: a page list with undo and redo, and a version number that
  tracks whether the document is modified
- [x] Edits in the model: delete, move, rotate and insert pages
- [x] Desktop: rotating, with undo and redo named in the menu
- [x] Desktop: deleting pages
- [x] Desktop: moving pages by dragging thumbnails
- [x] Desktop: documents with pages from several files, and Insert Pages from
  File
- [x] Saving as a full rewrite, with the result read back and checked page by
  page. On the corpus, 1,066 of 1,077 files come out identical, 11 are refused
  and none is damaged
- [x] Page labels kept through edits, in the apps and in saved files
- [x] Desktop: selecting several thumbnails
- [x] Desktop: PDFs dropped on the thumbnails are inserted there
- [x] Desktop: dragging pages between windows
- [x] Desktop: Save and Save As, and the question about unsaved changes
- [x] Desktop: Export Selected Pages
- [x] Property test for undo and redo over random edit sequences
- [x] Round-trip tests for saving, checked with qpdf
- [x] Desktop: noticing that the open file changed on disk
- [ ] Save the 11 corpus files that are refused. lopdf can't read damaged
  cross-reference tables or page trees that hayro recovers, and one file has an
  encryption dictionary that lopdf encrypts wrongly. Building the objects from
  hayro's reading of the file would be one way.
- [ ] Merge resources shared between sources, such as fonts embedded in both of
  two merged files.
- [ ] Android: notice a changed file when the app resumes, and reload or ask,
  like the desktop.
- [ ] A memory-mapped file that another program truncates in place crashes the
  app when the missing part is read. Programs that save by renaming, as most
  do, are safe. See the task on mapped files that shrink in M7.

## M4: Markup

- [x] The annotation model, and reading the markup a file has
- [x] Annotation edits in the edit model, on the same undo history as page
  edits
- [x] Desktop: Highlight, Underline and Strikethrough on the selected text,
  saved as annotations
- [x] Desktop: annotations a file already has can be picked and deleted. The
  page is then rendered from a copy without the annotation
- [x] Annotations the app draws are also in thumbnails and in print, and go
  along with pages dragged to another window
- [x] Desktop: the markup toolbar, with a color per tool
- [x] Desktop: drawing tools for freehand, rectangle, ellipse and line
- [x] Saving checks that a changed or deleted annotation is where it was found
- [x] Desktop: text boxes and notes
- [x] Desktop: typing a text box on the page, in its font, size and color
- [x] Desktop: moving and resizing drawings and shapes
- [x] Markup written by Specto is recognized when a file is opened again, so it
  can still be changed
- [x] Desktop: a list of all markup in the sidebar
- [x] Desktop: signatures, drawn or taken from a photo, kept with the app's
  data and placed with a click
- [x] Desktop: font, bold, italic and size for text boxes, in the three font
  families no PDF needs to embed
- [x] Android: markup shows in the thumbnails
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
- [ ] Android: build the app with the annotation edit kinds that were added to
  the shared code without a Kotlin build.
- [ ] Squiggly underlines drawn as a wavy line.
- [ ] Text follow-ups: alignment, typing on pages shown rotated, and any font
  of the system together with characters beyond Windows Latin. Both of those
  need a font embedded in the file and are one piece of work. Such characters
  are saved as question marks now.
- [ ] A text box being dragged shows only its frame until it is dropped. Draw
  its text in the overlay during the drag.
- [ ] Adjusting follow-ups: handles on the sides, Shift to keep proportions,
  arrow keys to nudge, snapping, and moving several at once.
- [ ] Incremental save, used when only annotations or form values changed. The
  lopdf evaluation shows how, also for encrypted files (`spikes/lopdf-spike` at
  commit 7273d64).
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

- [x] The forms crate: the field tree, inheritance, widgets by page, flags
- [x] Filled-in values in the edit document, with undo, and saved with new looks
- [x] Desktop: checkboxes, radio buttons and typing into text fields
- [x] Desktop: Tab between fields, Space and Enter on the field in focus
- [x] Menus and lists to choose one option from
- [x] Comb fields, hidden passwords, and text fields on turned pages
- [x] Android: filling in boxes, radio buttons, text fields, menus and lists
- [x] Values a file leaves to the reader to draw are shown
- [x] Menus that take typed text, and lists that take several choices
- [x] Comb fields with their letters in their cells while typing
- [x] Tab order by rows or columns, highlighting fields, and resetting a form
- [x] Android: typed menu choices, lists of several, highlighting and resetting
- [ ] An embedded fallback font for text outside Latin.
- [ ] Android: typing into a text field on the page instead of in a dialog,
  and a way from one field to the next from the keyboard.
- [ ] Forms of inserted files. Only the form of the opened file is kept.
- [ ] Saving filled forms incrementally, and an optional flattened export.

## M6: Android

- [x] The view session over the viewer and the scheduler, through UniFFI
- [x] The session crate: one type for the open document, shared by both apps
- [x] Editing commands through the FFI: rotate, delete, move, undo, redo and
  saving
- [x] Tiles: Kotlin owns a bitmap per tile and fills it through JNI, with the
  cache bookkeeping in Rust
- [x] The Compose viewer: scrolling with fling, pinch and double-tap zoom, the
  page indicator, and rotation without reloading
- [x] The work done on every frame moved from UniFFI to JNI
- [x] Kotlin unit tests, in CI
- [x] UI tests for most of the feature list, on a phone and a tablet emulator
- [x] UI tests in CI, for pushes to the main branch
- [x] Release build measured on a phone
- [x] CI lints the JNI code that only compiles for Android
- [x] Top bar with the page grid, the overflow menu and the password prompt
- [x] One page at a time with swipes, as the default on phones
- [x] Printed page numbers
- [x] Turning the page returns to the fitted view
- [x] Thumbnail strip along the bottom
- [x] Full screen
- [x] Scrolling the strip flips through the document
- [x] Outline, as a second tab beside the page grid
- [x] Links
- [x] Tablet layout, with a sidebar on wide windows
- [x] Two pages side by side on wide windows
- [x] Keyboard navigation
- [x] Tablet polish: the indicator names both pages, and the sidebar is
  remembered
- [x] Opening from the file picker and from other apps
- [x] Reading position and recent documents
- [x] Sharing the open document
- [x] Opening through a file descriptor, without reading the file into the
  app's memory
- [x] Save as
- [x] Save, over the file the document was opened from
- [x] Page organizer: picking pages, rotating and deleting them
- [x] Page organizer: dragging to reorder
- [x] Text markup: highlighting, underlining and striking through the selected
  text, and deleting a picked annotation
- [x] Text selection with handles, and copying
- [x] Sharing the selected text
- [x] Search
- [x] Document properties
- [x] The view button and its menu, with fit page and fit width
- [x] Settings screen
- [x] Setting: keep the screen on while reading
- [x] Setting: volume keys turn pages
- [x] ktlint formats the Kotlin code and checks it in CI
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
- [ ] Unsaved edits should survive the app being ended in the background. Keep
  the edited page list in the document store and offer it again when the
  document is next opened.
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
- [ ] Trim margins: a switch in the view menu, remembered per document. Depends
  on the task in M1.
- [ ] Bookmarks in the Pages and Contents view. Depends on the task in M1.
- [ ] Document properties follow-ups: how the file is protected, and its fonts.

## M7: Polish and hardening

- [x] Printing on the desktop, with pages rendered at 300 dpi
- [x] Printing on Android, by handing the document to the system's print
  service
- [x] Password prompt when opening encrypted PDFs, in both apps
- [x] Flatpak, checked in the sandbox
- [x] Android: release builds signed with a key of their own
- [x] Android: code shrinking in the release build, which took the APK from
  16.4 to 9.4 MB
- [x] The app's name and ID settled: Specto, `net.xfbs.specto`
- [x] Desktop integration files: the launcher entry and the AppStream metainfo
- [x] The icon: a page under a magnifying glass
- [x] Desktop: the About dialog
- [x] cargo-deny in CI, for security advisories and license compatibility
- [x] Desktop: large files opened without reading them into memory
- [x] Android: a crash log, which Settings shows
- [x] Android: a tag builds, signs and publishes the APK as a GitHub release
- [x] A tag builds Flatpak bundles in CI and attaches them to the same release
- [ ] Vector printing on the desktop: send the pages themselves instead of
  images, for smaller files when printing to a file and for selectable text.
- [ ] Encrypted incremental saves, once the incremental save exists. The full
  rewrite already keeps a document's encryption (D9).
- [ ] Permission flags: honor the restrictions of an owner password on editing,
  annotating and printing, with a prompt for the owner password to unlock them.
  A setting in both apps turns this off (D9). What a restriction blocks should
  say so and point to the setting.
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
  [packaging/README.md](../packaging/README.md)). The metainfo and the script
  that writes the Flathub files are ready. Left to do: a tagged release, and
  the pull request to Flathub.
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

- [x] Desktop: the app's texts go through gettext, in English only so far
- [x] Android: the app's texts are string resources
- [x] Android: a German translation, with a unit test that holds every
  translation to the English texts
- [x] Desktop: the translation catalogs are built and installed, also in the
  Flatpak, with a headless check that starts the app in another language
- [x] A Dutch translation of both apps
- [ ] Core: no text for users from the crates. The frontends map error kinds to
  their own translated messages instead of showing the crates' English text.
- [ ] Numbers through the locale: page counts and the zoom percentage.
- [ ] Desktop: translations on macOS. Check the build with Homebrew's gettext,
  which nobody has tried yet, and install the catalogs there.
- [ ] CI: fail when the translation template isn't what its script writes.
- [ ] Desktop: a German translation.
- [ ] Right-to-left UI languages: check both apps under such a locale. GTK and
  Compose mirror on their own, but the sidebar side, the thumbnail strip and
  icons with a direction need a look. The page canvas does not mirror.
- [ ] Binding direction: one property in the viewer for both the order of pages
  in a two-page spread and the swipe direction. It defaults to what the
  document states, independent of the UI language, with a toggle per document,
  since many right-to-left PDFs don't state it.
- [ ] Decide how translations are contributed: plain files in the repository,
  or a hosted tool such as Weblate.
