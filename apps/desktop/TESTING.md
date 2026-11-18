# Testing the desktop app

The app is tested in two ways. Automated checks drive a headless instance of
the app and cover much of [FEATURES.md](FEATURES.md), the list of what the app
does and what is expected to work. The rest is tested by hand, with the setup
and the specific checks further down.

Before you start, install the build prerequisites first, as explained in
[BUILDING.md](BUILDING.md).

## Automated checks

The automated checks run in CI, and locally on Linux. They need mutter. On
Fedora, there is a script to install the packages:

```
ci/install-deps-fedora.sh --headless
```

To run the automated checks, use the provided script:

```sh
cargo build -p specto-desktop
apps/desktop/test-ui/run.sh python3 -I apps/desktop/test-ui/check.py
# At another scale, also 1.25.
SPECTO_SCALE=2 apps/desktop/test-ui/run.sh \
  python3 -I apps/desktop/test-ui/check.py
# Some tests only.
apps/desktop/test-ui/run.sh \
  python3 -I apps/desktop/test-ui/check.py zoom layouts
```

The app runs in a headless compositor with its own saved state, virtual pointer
and virtual keyboard, so your own session isn't touched. What's covered is
listed at the top of `apps/desktop/test-ui/check.py`.

Not covered, and left to the manual pass:

- Pinch zoom
- The file, password and print dialogs
- Dragging thumbnails (the checks drop them through a test hook)
- How sharp the text looks
- How selecting feels with a real mouse or touchpad
- macOS

## Setup

All commands and paths are relative to the repository root.

```sh
cargo build --release -p specto-desktop
target/release/specto-gtk                 # start page
target/release/specto-gtk a.pdf b.pdf     # one window per file
```

On macOS, header bar and sidebar icons are missing if the Adwaita icon theme
isn't installed.

*Primary* means Ctrl on Linux and Cmd on macOS.

### Test files

Run these once. The second needs qpdf.

```sh
corpus/fetch.sh
tools/test-files.sh
```

This puts copies of the test files in `target/test-files/`, so saving doesn't
change the corpus. Run `tools/test-files.sh` again for fresh copies. The checks
below refer to the files by these names:

- LONG (`long.pdf`): 352 pages, an outline, web links, three page sizes.
- LINKS (`links.pdf`): in-document links.
- LABELS (`labels.pdf`): page labels i, ii, iii, iv, 1, 2, 3, a, b, 4, 5.
- ROTATED (`rotated.pdf`): every page rotated.
- SIZES (`sizes.pdf`): pages of different sizes, no outline.
- GIANT (`giant.pdf`): one 4.4 by 5.9 m page with a 212-megapixel image, slow
  on purpose. It needs about 0.7 GB at fit width and up to 4.5 GB zoomed in to
  100%.
- BROKEN (`broken.pdf`): not a valid PDF.
- ENCRYPTED (`encrypted.pdf`): LABELS with the password `secret`.
- BIG (`big.pdf`): LONG padded to over 64 MB, so it is memory-mapped when
  opened.

### Resetting saved state

- View state: delete `~/.local/state/specto/view-state.ini` (or
  `$XDG_STATE_HOME/specto/view-state.ini`). The path is the same on macOS.
- Recent files: they live in GTK's shared list
  (`~/.local/share/recently-used.xbel`), which other apps use too, so don't
  delete it. Clear the recent history under Privacy in GNOME Settings, or test
  with files you haven't opened before.

## Specific checks

### Opening files

- ENCRYPTED: a wrong password brings the dialog back saying it was incorrect.
  Cancel opens nothing. The right password opens it, with the labels of LABELS.
- A recent file that was moved or deleted disappears from the start page after
  a restart.
- BIG opens at once, and the app's memory use stays well below the file's size.

### Viewing

- ROTATED: every page is shown the right way up, in its rotated orientation.
- SIZES and LONG: pages of different sizes are centered, with equal gaps
  between them.
- Show Cover Page Alone: page 1 alone, then 2 and 3, 4 and 5, and so on.
- Trim Margins on a book or a paper in the single page layout: each page loses
  its margins at once, turning pages shows no delay, and a cover or full-page
  figure is shown whole. Text selection, links and markup land where the
  pointer is. In a continuous layout the pages are whole and the switch is
  grayed out. Thumbnails stay whole.
- Resizing the window at fit width keeps the view at the same place in the
  document.

### Zoom and sharpness

- Ctrl+scroll and pinch: the point under the pointer or fingers stays put.
- Stopping at an odd zoom such as 137%: slightly soft for about a quarter
  second, then sharp.
- Holding a pinch without lifting the fingers: stays soft until released.
- On a HiDPI screen (Retina, or Linux at scale 2): text at fit width is as
  crisp as at 100%, and as crisp as in other apps. Thumbnails are sharp too.
- Linux with fractional scaling (125% or 150%) and a window that isn't
  maximized: just as crisp.
- Moving the window to a screen with a different scale: it re-renders sharp
  there.

### Navigation

- Typing 999 or "abc" in the page entry: an error bell.
- LONG has Thumbnails, Contents and Markup tabs. SIZES, which has no outline,
  has no Contents tab.
- Contents entries in LONG jump to the exact place, often mid-page. Long titles
  wrap to up to three lines and are then cut off with an ellipsis.
- Page numbers in Contents match the printed ones. A good file for this on
  Fedora is `/usr/share/doc/hwloc/hwloc-a4.pdf`, which starts its numbering
  after roman front matter.
- LONG: web links open in the browser.
- Pressing on a link, moving a few pixels and releasing still follows it.
- LINKS: rest the pointer on an in-document link. The panel showing the place
  it leads to appears after half a second, sharp, with the target near the top.
  It goes when the view scrolls. Moving across several links in a row shows
  nothing until the pointer rests.
- A link near the bottom of the window: the panel shows above the link, not cut
  off.
- LONG: web links show no panel.
- LINKS: scroll to the middle of a page and click a link. Alt+Left (Cmd+[ on
  macOS) returns to the same place mid-page. The back and forward buttons of a
  mouse go back and forward too.
- LONG: click a Contents entry, type a page number, then search for a word and
  step through several matches. Alt+Left goes back through them one by one:
  to where the search began, to the Contents entry's target, and to the start.
- Scrolling leaves nothing to go back to.

### Text

- LONG: select a paragraph, copy it and paste it into a text editor. The text
  matches, with line breaks at the ends of lines. Words hyphenated at a line
  end stay hyphenated.
- A two-column paper: dragging within a column stays in that column. Dragging
  from one column into the other selects the rest of the first column and the
  start of the second.
- Dragging quickly past the bottom of a window that isn't maximized: it keeps
  scrolling while the pointer is outside, faster the further out.
- Linux: after selecting, a middle click in a text editor pastes the selection,
  without pressing *Primary*+C.
- Typing in the page entry and pressing *Primary*+A or *Primary*+C acts on the
  entry's text, not the document.
- In dark mode the selection is still readable on the white page.

### Search

- LONG: search for "copyright". Matches appear on the current page or the next
  ones first, and the count grows until it settles after a second or so. Enter
  steps through the matches, scrolling only when the match is off screen.
- LONG: a phrase that wraps onto the next line in the book is highlighted on
  both lines.
- Opening the bar with *Primary*+F and typing straight away: nothing typed is
  lost.
- With matches on screen, zooming and switching layouts: the highlights stay on
  the words.
- GIANT: searching doesn't freeze the window. The page has no text, so the
  result is "No matches".

### Editing

Rotating:

- LONG: a page turned with *Primary*+Right is sharp after a moment, and its
  thumbnail turns too.
- On a rotated page, search highlights lie on the words, and links and
  Contents entries still work.

Deleting:

- With every thumbnail selected, Delete beeps.
- Delete Pages in the main menu deletes the current page.
- Typing in the search field, Delete and Backspace edit the text and delete no
  pages.
- LONG with pages deleted, saved and reopened: the pages are gone. Contents
  entries that pointed at them do nothing, and the others still work.

Moving:

- LONG: drag thumbnail 2 below thumbnail 4. While dragging, a line in the
  accent color shows the place between thumbnails, and the drop puts the page
  there.
- Select thumbnails 2 and 3, and drag one of them down past the bottom of the
  sidebar. The sidebar scrolls, and dropping moves both ("Move 2 Pages").

Inserting:

- LONG, on page 2: Insert Pages from File, choosing LINKS in the file dialog.
  The in-document links of LINKS lead to its own inserted pages.
- Inserting ENCRYPTED asks for its password. Cancel inserts nothing.
- With a search open, inserting a file: matches on the new pages are found too.
- LONG with LINKS inserted, saved and reopened: the Contents of LONG still
  work. The file opens in other viewers.
- Drag two PDFs from the file manager onto the thumbnails, between pages 3 and
  4. A line shows the place, then their pages are inserted there. Dropped on
  the page view instead, they open in new windows.
- A non-PDF file dropped on the thumbnails gives the "not a valid PDF file"
  message and inserts nothing.
- LONG and LINKS in two windows: drag two thumbnails of LONG onto the
  thumbnails of LINKS. A line shows the place, and they're inserted where
  dropped.

Page labels:

- LABELS: delete "ii" and move "1" to the end. The page entry and thumbnails
  show i, iii, iv, 2, 3, a, b, 4, 5, 1. Saved and reopened, here and in another
  viewer, the labels are the same. Typing "1" goes to the last page.

Undo and shortcuts:

- The main menu shows "Undo Rotate Page" and "Redo Rotate Page" as appropriate.
- Typing in the page entry, *Primary*+Z undoes the typing. No edit is undone.

### Markup

- LONG: select a few words and press Shift+*Primary*+H. They turn yellow, the
  text under the yellow stays black and sharp (not grayed or washed out), and
  the selection is gone. Zoom in and out and scroll: the highlight stays
  exactly on its words, also while zooming.
- Select from the middle of one line to the middle of the next and highlight:
  each line is covered just as far as it was selected.
- Underline (Shift+*Primary*+U) and Strikethrough (Shift+*Primary*+X): a red
  line under and through the words, at a sensible height for the text size.
- Select text over a page break and highlight: both pages are marked, and one
  *Primary*+Z takes both away.
- ENCRYPTED: highlight, save and reopen with the password. The highlight is
  there.

Picking and deleting:

- A click on a link under markup still follows the link.
- ROTATED: highlights lie on the text there too. Rotate a page with a highlight
  (*Primary*+Right): the highlight turns with it.
- Move a page with a highlight in the sidebar, and delete one and undo: the
  highlight stays with its page.
- Delete markup the file already has: nothing else on the page changes or
  moves, and the thumbnail follows.
- A file with other programs' annotations (`freetexts.pdf`, `inks.pdf` and
  `comments.pdf` in `corpus/files/pdfjs/test/pdfs/`): each can be picked and
  deleted, one at a time, and the others stay as they were. Deleting one that
  has a reply or a popup takes those along.
- LONG, a highlight deleted on one page: scrolling, zooming, selecting text,
  links and search on that page work as before.

The markup bar and colors:

- The markup bar (the pencil in the header bar): scrolling and links still work
  with a tool pressed in.
- Choose green with Highlight pressed in, switch to Underline and back. The
  bar shows green again for Highlight.
- Pick a highlight and choose another color: *Primary*+Z changes it back. The
  same works for a highlight the file already had, and nothing else on the
  page changes.
- The color button shows the color in use. Its swatches are easy to tell apart
  and to hit, in the light and the dark style. The icons of the three text
  tools are clear enough to tell apart without their tooltips.
- The markup bar is above the page view only, and the sidebar keeps its full
  height. In a narrow window nothing of the bar is cut off.

Drawing:

- Freehand: draw a signature-like scribble with the mouse and with a touchpad.
  The line follows the pointer without lag, looks smooth and not angular, and
  stays the same after the button is let go, with no visible jump when it is
  thinned out.
- Draw across the edge of a page: the stroke stops at the edge. Draw with the
  page zoomed far in and far out: the line keeps its thickness relative to the
  page.
- Rectangle, ellipse and line: the shape follows the pointer while dragging, in
  any direction. Escape during a drag gives it up.
- With a drawing tool pressed in, scrolling with the wheel or touchpad still
  works.

Moving and resizing:

- Move a shape by dragging it, and resize it by its handles. It follows the
  pointer without lag or trailing, the handles are easy to see and to take, and
  the pointer changes shape over them. A drag that starts on text next to a
  shape still selects text.
- Resize a freehand stroke by a corner: it scales, and can be flipped by
  dragging the corner past the opposite one.
- Save, reopen, and move a shape again. Nothing else on the page flickers or
  shifts when the drag starts.

Text boxes and notes:

- Text tool: click and type. The letters appear on the page as they are typed.
  Type a few lines with line breaks and a long line that wraps, then press
  Escape or click elsewhere. The text does not jump or reflow when typing
  ends: same place, same size, same line breaks. Compare closely at 100% and
  zoomed far in.
- Double click the text box and change it. While typing, the old text is gone
  from the page, not doubled behind the field.
- Scroll and zoom while typing: the field stays on its box. *Primary*+A,
  *Primary*+C, *Primary*+V, *Primary*+Z and *Primary*+Left and Right act on the
  text, not on the document.
- Typing near the right and bottom edge of the page view and of the window.
- Font and size: with the text tool, choose Serif, bold and 24, then type. The
  field shows that font while typing, and the finished box looks the same.
  Change the font and size while typing: the field follows and keeps the
  keyboard. Pick a finished box and change them: it changes, and one undo
  changes it back.
- The font menu shows each name in its own font. The three fonts look right in
  the finished box and in other programs: a sans-serif, a Times-like serif and
  a typewriter font, each with bold and italic.
- Compare line breaks while typing and after, for each font, with a paragraph
  long enough to wrap several times.
- A narrow window: the bar scrolls sideways and nothing in the window moves
  when the text controls appear.
- Type umlauts, quotes and a euro sign: they show. Characters outside Western
  European (Greek, Cyrillic, CJK, emoji) show as question marks.
- Note tool: resting the pointer on a note shows its text as a tooltip. The
  editor is roomy enough for a few lines. Dragging a note still moves it
  without opening it.
- Pick a highlight and press Enter: a comment can be typed for it. It shows in
  other programs' annotation lists after saving.
- Notes and comments still use the small editor, which points at the
  annotation, is not cut off near the window's edges, and takes the keyboard
  at once.
- Move and resize a text box. While it is dragged only its frame moves, and the
  text follows when it is dropped.
- Text boxes and notes after saving and reopening: in other programs the text
  shows in the same place and size, and can be edited there.

Markup list:

- The Markup page of the sidebar on `comments.pdf` and `freetexts.pdf` from the
  corpus: every annotation on the pages is in the list and the other way
  round. A click on a row scrolls to the annotation and frames it. Rows with
  long text are cut after three lines and show all of it as a tooltip.
- Add, change, delete and undo markup with the list open: it follows at once
  and keeps its scroll position.
- Backspace in the list with the markup bar closed, and after moving through
  the rows with the arrow keys: one annotation goes per press, and no page is
  ever deleted.
- LONG with the Markup page open: opening the document and editing stay quick.

Signatures:

- New Signature, Draw: sign with the mouse and with a touchpad. The line
  follows without lag and looks like handwriting. Clear empties the pad. Save
  with an empty pad does nothing.
- New Signature, From a Picture: sign on white paper, take a photo with a
  phone (try one with a shadow across it), and choose it. The preview shows
  the ink alone. Move the slider: to the left faint strokes and some of the
  paper's shadows appear, to the right only dark strokes stay. Somewhere in
  between the paper is gone and the strokes are whole. Try the three ink
  colors. A JPEG, a PNG and a HEIC from a phone all open, or say so clearly.
- Place a saved signature on a form's signature line. Its edges are clean
  against the page (no pale box around it, no halo), also zoomed far in and on
  a colored background.
- The signature list: each entry is recognizable, deleting one removes it, and
  the list is the same in another window and after a restart.
- Save and reopen the document, then print it: the signature is there each
  time, sharp. Other programs show it in the same place.
- Flatpak: saved signatures survive a restart, and choosing a picture works
  through the file chooser.

Saving, printing and other windows:

- Save, close and open again: markup, drawings and shapes look the same as
  before saving. Compare the color, the position and the height of the lines
  closely.
- The saved file in other programs (Papers or Evince, Firefox, Preview on
  macOS): the markup shows, in the same place and color, and can be selected,
  moved and deleted there. In a list of annotations, text markup shows the
  marked text.
- Thumbnails show new markup at once, and lose it on undo.
- Print with unsaved markup (to a file is enough): it is in the same place on
  the printed page as on screen.
- Two windows: drag the thumbnail of a marked page into the other window's
  sidebar. The markup is there too, and one undo there removes the page and its
  markup.

### Forms

A form with text fields, checkboxes and radio buttons is needed, such as a tax
or application form.

- Type into a text field: the text stands where it does after Enter, in the
  same size, with no jump. Check a centered or right-aligned field and a field
  of several lines too.
- Zoom and scroll while typing: the text field stays on its place.
- Fill in a whole form without the pointer after the first click: Tab and
  Shift+Tab go through the fields in a sensible order, over page breaks too,
  the view follows, and a frame shows where the keyboard is.
- After Escape, Tab moves through the window's controls as usual.
- Click a menu and a list of a form: what they offer shows at the field, long
  lists scroll, and what is chosen shows in the field afterwards. In a list
  the chosen row is marked and in view.
- Open `corpus/files/pdfjs/test/pdfs/bug1883609.pdf`: the gender menu shows
  "Man" and the color menu "Red" right away. Choose another and then the first
  again: it shows again.
- A list that takes several choices: rows can be ticked by click too, and the
  list on the page marks every ticked row.
- A password field shows asterisks once typing ends.
- A comb field (boxes for single letters, as for a date or a postal code): zoom
  while typing, and the letters stay in their boxes.
- A form whose page asks for a tab order by rows: Tab goes along each row
  from the left, then to the row below.
- Highlight Form Fields in the main menu tints the fields in the light and the
  dark style, and not the read-only ones. It and Reset Form are grayed out for
  a file without a form.
- Turn a page with Rotate Right and click a text field: the small editor beside
  the field has the cursor after what the field holds. Saved, the text stands
  in the field the way the field is turned.
- A form whose pages are turned in the file (a landscape form): filled-in text
  reads upright, not sideways.
- Undo and redo go through the fields in order.
- The saved file in other programs (Papers or Evince, Firefox, Preview on
  macOS, Acrobat if at hand): the values show, and can be changed there.
- Thumbnails and a print show what was filled in.

### Saving

- LONG: rotate a few pages and save. Reopened, the Contents, links and printed
  page numbers still work. The file opens in other viewers too (Evince or
  Papers on Linux, Preview on macOS).
- ENCRYPTED: rotate a page and save. Reopening asks for the password again, and
  `secret` still works.
- LONG: highlight a word and save. The file is a little larger than before, not
  smaller, and it opens in other viewers with the highlight. Delete the
  highlight and save again: the file no longer contains it, which the command
  below shows by printing 0.

```sh
# The number of highlights in the file.
qpdf --qdf --object-streams=disable target/test-files/long.pdf - \
  | grep -c "/Subtype /Highlight"
```

- LONG: select thumbnails 2 to 4 and 7, then Export Selected Pages. The
  exported file opens in other viewers, with any rotated pages turned. With one
  page selected, only that page is exported.
- With changes, close the window with *Primary*+W: "Save Changes?" appears.
  With several edited windows, quitting asks for each one.
- Saving into a folder you can't write to: a message says it couldn't save,
  with "Permission denied", and the document stays "Edited".
- A file that is a symbolic link: after saving, the link is still a link,
  pointing at the updated file.
- GIANT: saving takes a few seconds and the window stays responsive meanwhile.

### Printing

Print to a real printer where you can. Otherwise "Print to File" (PDF) shows
the same.

- LONG: *Primary*+P, all pages to a file. Each page is centered on the paper,
  and the window stays responsive while it prints.
- Rotate page 2, then print the current page (page 2): it's printed turned, on
  landscape paper.
- A range such as 10-12 prints those pages.
- With pages inserted from another file, they're printed in their place.
- Print again: the dialog starts with the printer and settings used last time.
- GIANT: prints (slowly), at the paper size, without freezing the window.

### Properties

- LONG: Alt+Enter (Cmd+I on macOS) or Properties in the main menu. The dialog
  shows the title and the other things the file states.
- The dates are in your language and on your clock.
- A value can be selected with the mouse and copied.
- ENCRYPTED: the protection is "Password to open".
- A file that restricts printing or copying, where you have one: "Not Allowed"
  lists what it restricts.
- After deleting a page (unsaved), the dialog shows the new number of pages.
- On the start page, Properties is disabled.

### Changes by other programs

Use a copy of LONG, and another program that saves PDFs (qpdf, a PDF editor, or
just `cp`). This replaces the file with page 50 turned:

```sh
qpdf --rotate=+90:50 long.pdf --replace-input
```

- LONG open on page 50, unedited: replace the file. The new version is shown at
  the same zoom, with page 50 now turned.
- The same with an edit: save, and in "File Changed on Disk" choose Save
  Anyway. It replaces the other program's version.
- ENCRYPTED, replaced by another encrypted copy with the same password: it is
  reloaded without asking. With a different password, the password dialog comes
  up.

### Flatpak

Build and install it with `packaging/flatpak/build.sh` (see
[packaging/README.md](../../packaging/README.md)), then start it from the GNOME
app grid or as below. The app sees only the files it's given, so these go
through the document portal.

```sh
flatpak run net.xfbs.specto
```

- The app grid shows it with its icon. GNOME Files lists it under "Open With"
  for PDFs, and opening a PDF that way works.
- Open a PDF with the Open dialog, from GNOME Files, and by dropping it on the
  window. The start page later lists it with its real folder, not one under
  `/run/user`, and the Save As and Export dialogs start in that folder.
- Rotate a page, save, and open the file in another viewer: the page is turned.
  There are no leftover hidden files next to it.
- Insert Pages from File, Export Selected Pages and Print work like outside the
  sandbox.
- Change the file with another program while the window is in the background,
  then switch back to it: the new version is shown, or the banner if there are
  edits.

### Remembering state

- LONG: go to the middle of page 200, close and reopen. The view is at the same
  spot on page 200.
- Fit width, closed and reopened in a different window size, is still fit
  width.
- With three documents open, *Primary*+Q: all three remember their state.

### Slow pages

- GIANT: a spinner on a small rounded card over the page while it renders, gone
  once the page is sharp (a few seconds). Meanwhile menus, resizing and
  scrolling stay responsive.
- GIANT and LONG open together: LONG scrolls and renders normally.
- Normal documents never show a spinner.

## Platform notes

macOS:

- The app has a menu bar (File, Edit, View, Go, Tools, Help) in the system menu
  bar, with the shortcuts shown. GTK draws its own window decorations.
- Shortcuts work with Cmd, and also with Ctrl.
- Back and Forward are in the Go menu, disabled when there is nowhere to go.
- Copy and Select All in the Edit menu work, and so do Cmd+C and Cmd+A.
- Find, Find Next and Find Previous in the Edit menu work, and so do Cmd+F,
  Cmd+G and Shift+Cmd+G.
- Undo and Redo in the Edit menu and Rotate Left and Rotate Right in the Tools
  menu work, and so do Cmd+Z, Shift+Cmd+Z, Cmd+Left and Cmd+Right.
- Print in the File menu, and Cmd+P. Whether real printers appear depends on
  how Homebrew's GTK was built, so note what the dialog offers.
- Opening PDFs from Finder with "Open With" doesn't work, since there's no app
  bundle. Use the command line, the dialog or drag-and-drop.
- Retina screens are scale 2.

Linux:

- Fractional scaling needs to be enabled under Displays in GNOME Settings.
- Recently opened PDFs also appear in GNOME Files under Recent.

## Reporting

For each failure, note:

- the feature (section and bullet in FEATURES.md) or check (section here)
- the platform and OS version, such as "macOS 26.1, M2" or "Fedora 44, GNOME
  49"
- what happened instead, with a screenshot if it's visual

For crashes, run the app from a terminal and include the output.

The "Not yet" section of FEATURES.md lists what's missing on purpose. Those
aren't bugs.
