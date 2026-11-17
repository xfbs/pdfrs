# Desktop app features

What the desktop app currently does. For testing these by hand, see
[TESTING.md](TESTING.md).

Shortcuts use *Primary*, which is Ctrl on Linux and Cmd on macOS.

## Opening files

- Start page with an Open button and a list of recently opened PDFs
- Open with the file dialog (*Primary*+O), from the command line, or by
  dragging a file onto a window
- One window per document. Opening a file while a document is shown opens a new
  window
- Password prompt for encrypted PDFs, with a retry on a wrong password
- Error message for files that aren't valid PDFs
- Large files (64 MB and up) are mapped into memory rather than read, so they
  open at once and only the parts in use take memory
- Opened files are added to the system's recent files (shown in GNOME Files
  too)

## Viewing

- Continuous scrolling with native overlay scrollbars and kinetic touchpad
  scrolling
- Layouts: single page, continuous, two pages, two pages continuous
  (*Primary*+1 to 4), optionally with the first page alone (book cover)
- Rotated pages and mixed page sizes shown correctly
- Sharp on HiDPI and fractionally scaled screens: once zooming stops, pages are
  drawn at exactly the screen resolution
- Low-resolution previews while pages render, then sharp tiles
- A spinner over pages that take long to render (huge images, very complex
  pages)

## Zoom

- Zoom in and out (*Primary*+Plus and *Primary*+Minus), actual size
  (*Primary*+0), fit width (*Primary*+8) and fit page (*Primary*+9), also in
  the zoom menu
- Ctrl+scroll (also Cmd+scroll on macOS) and touchpad pinch, zooming around the
  pointer
- Zoom range 5% to 3200%. The current zoom is shown in the header

## Navigation

- Page entry showing the current page. Type a page number to go there
  (*Primary*+L)
- Page labels such as "iv" or "A-3" in the page entry and thumbnails. Labels
  can be typed too
- Keyboard: Page Down, Space, Page Up and Backspace scroll by a screenful in
  the continuous layouts and turn the page in the others. Left and Right turn
  the page, Up and Down scroll, Home and End go to the first and last page
- Thumbnail sidebar (F9), with the current page highlighted and kept in view
- Several thumbnails can be selected: Shift+click for a range, *Primary*+click
  to add or remove one, *Primary*+A with the sidebar focused for all. A
  selection of several stays while you move around the document, and a plain
  click selects one page again and goes there
- Contents tab with the document's outline, when it has one. Titles wrap to up
  to three lines, with the page number (or label) right-aligned in gray, and
  entries jump to the exact position
- Links: in-document links and named actions (such as "next page") are
  followed, web and mail links open in the default app, and other kinds are
  refused
- Back and Forward (Alt+Left and Alt+Right, Cmd+[ and Cmd+] on macOS, or the
  back and forward buttons of a mouse): after a link, a Contents entry, a page
  typed into the page entry, the first or last page, a search result or an
  entry of the Markup list, Back returns to the place left behind, and Forward
  undoes it. A search counts once, from where it began. Turning pages,
  scrolling and thumbnails aren't remembered. The last 100 places are kept

## Text

- Drag over text to select it, across lines and pages. The selection is drawn
  in the accent color
- Double-click selects a word, triple-click a line. Dragging afterwards extends
  by words or lines
- Shift+click extends the selection
- An I-beam cursor over text and a hand over links. Clicking a link follows it,
  dragging from one selects
- Dragging near the top or bottom edge of the view scrolls
- *Primary*+C copies the selection, *Primary*+A selects the whole document,
  Escape or a click clears the selection
- On Linux the selection is also the primary selection, for middle-click paste
- Works on rotated pages and rotated text

## Search

- *Primary*+F or the search button opens a search bar under the header bar
- Finds text regardless of case and accents ("cafe" finds "Café"), across line
  breaks, and words hyphenated at the end of a line
- Shows "3 of 17", or "No matches". Enter and Shift+Enter (or *Primary*+G and
  Shift+*Primary*+G, or the arrow buttons) go to the next and previous match,
  wrapping around at the ends
- The first match is the first one from the current page on. The view only
  scrolls when the match isn't already visible
- All matches are highlighted in yellow, the current one in orange
- Long documents are searched in the background: the first matches show up at
  once, and the count keeps growing until every page is done
- Escape closes the bar and removes the highlights

## Editing

- Rotate Left and Rotate Right (*Primary*+Left and *Primary*+Right, or the main
  menu) turn the pages selected in the sidebar if there are several, otherwise
  the current page, or both pages of a two-page spread
- Delete Pages (the main menu, or Delete or Backspace on the thumbnails)
  deletes the pages selected in the sidebar if there are several, otherwise the
  current page. The last page can't be deleted. On the page view the keys
  delete no pages, so that a key hit by mistake costs nothing
- Dragging thumbnails moves pages, and dragging a selected thumbnail moves all
  the selected pages. A line between thumbnails shows where they'll go, and the
  sidebar scrolls when dragging near its top or bottom. The moved pages stay
  selected, and the view stays on the page it showed
- Insert Pages from File (main menu): all pages of another PDF go after the
  current page (or spread), and the view goes to the first of them, with the
  new pages selected in the sidebar. Encrypted files ask for their password
- Dropping PDFs (from the file manager) on the thumbnails inserts all their
  pages there, as one edit. Dropping them on the page view opens them in new
  windows instead
- Thumbnails dragged into another window's sidebar are copied there (with their
  rotation), as one edit. The first window keeps them
- Undo and Redo (*Primary*+Z, Shift+*Primary*+Z, or the main menu, which names
  the edit: "Undo Rotate Page")
- "Edited" under the title while there are changes. Undoing back to the state
  as opened removes it
- Selection, search, links and the table of contents keep working on edited
  pages, inserted ones included
- Pages keep their printed page numbers (labels) through rotating, deleting,
  moving and inserting. Inserted pages of a file without labels are numbered by
  their position
- In the page entry or the search field, the shortcuts act on the text instead
  (Undo, moving by words)

## Markup

- The pencil button in the header bar (or Shift+*Primary*+A) shows the markup
  bar above the page view: the text tools (Highlight, Underline and
  Strikethrough), the drawing tools, the signature button, and a color button
  with seven colors, black among them
- A text tool that is pressed in marks text as it is selected, by dragging or
  by double and triple clicks, until it is let go of (press it again, or close
  the bar). Pressing one in with text selected marks that text
- Without the bar, Shift+*Primary*+H, U and X mark the selected text once
- A selection over several pages is marked on each, as one edit
- A highlight darkens the page beneath it like a highlighter pen, so the text
  stays black
- Drawing tools: freehand, rectangle, ellipse and line. With one pressed in, a
  drag draws instead of selecting text, and the pointer is a crosshair. A
  drawing stays on the page it started on, and Escape gives it up
- Freehand strokes are smoothed. A click with the pen makes a dot, and a click
  with a shape tool draws nothing
- Each tool has its own color, changed with the color button while the tool
  is pressed in. The button shows the color in use. The highlighter starts in
  yellow, and the lines and the drawing tools in red
- Text tool: a click on the page puts a text field there, and what is typed
  becomes a text box, in the font and size the bar is set to (Helvetica of 12
  points to begin with). The text appears as it will
  stand, in the same place, with the same line breaks. Escape or a click
  elsewhere ends typing. The box is as high as its text needs, and resizing it
  changes its width
- Font and size: with the text tool, or a text box picked, the bar shows a
  font menu (sans-serif, serif and monospace, each name in its font), bold and
  italic, and the size in points. They set what is typed next, and change the
  picked text box, also while typing into it. These are Helvetica, Times and
  Courier, the fonts every PDF program has, so nothing is embedded
- A double click on a text box, or Enter with it picked, types into it again.
  On pages shown rotated, a small editor is used instead
- Note tool: a click on the page puts a note there and asks for its text
- A note shows its text when the pointer rests on it, and a click on it opens
  the text to read or change. Comments on drawings, shapes and other programs'
  annotations show on hover too
- A double click on a text box or a note, or Enter with any markup picked,
  asks for its text again. For other markup that text is a comment, which other
  programs show in their annotation lists. A text box left empty is deleted
- Signature button on the bar: lists the saved signatures and makes new ones.
  Choosing one and clicking on a page places it there, 150 points wide. It can
  then be moved and resized like a shape, and keeps its proportions
- New Signature: drawn with the mouse or touchpad, or taken from a picture of
  a signature on white paper. For a picture, the paper is made transparent,
  also under uneven light, with a slider for how faint a stroke may be and a
  choice of ink color (as photographed, black or blue)
- Saved signatures are kept on this computer only, in the app's data
  directory, and can be deleted from the list. A placed signature is a picture
  on the page. It does not sign the document digitally
- A click on markup picks it (a frame shows around it). Delete or Backspace
  deletes it, Escape lets go of it. That goes for what the file already had
  too, of any kind (notes, text boxes, drawings, stamps), except what the file
  marks as locked or read-only
- Drawings and lines are picked by clicking on the line itself, shapes anywhere
  in their area
- With markup picked, the bar shows its color, and choosing another recolors
  it. For markup the file already had this works for text markup only, because
  other kinds can have details (arrowheads, dashed borders) that redrawing
  would lose
- A drag on a drawing or a shape moves it, whether it was picked before or not.
  The picked one has handles to resize it: on the corners of its frame, or on
  the two ends of a line. Escape during a drag puts it back
- Drawings and shapes saved by Specto can still be moved, resized and recolored
  after the file is opened again. Those from other programs can only be deleted
- Undo and Redo cover markup like page edits, in the order things were done
- Thumbnails and printed pages show markup as the page view does, saved or not
- Thumbnails dragged to another window take their pages' markup along
- Saved with the document as standard PDF annotations, which other programs
  show and can edit
- Markup a file has without a ready-made look (some programs leave that out) is
  shown where it is text markup
- The Markup page of the sidebar lists all markup of the document, also what
  other programs made: its color, its text (what was marked, a note, a
  comment), what kind it is, and its page. A click on a row picks the
  annotation and scrolls to it, and picking one on the page marks its row.
  Delete and Backspace in the list delete the annotation of the marked row, and
  never pages
- Not yet: line width, other fonts than these three, text alignment

## Forms

- A click ticks or unticks a checkbox, and chooses a radio button
- A click on a text field types into it where it is, in the field's font, size
  and color. Enter, Escape or a click elsewhere ends typing and keeps the text.
  In a field of several lines, Enter starts a new line
- A password field hides what is typed into it, and shows asterisks afterwards
- A comb field, which has a cell for each letter, has each letter in the
  middle of its cell, while typing too, and takes no more letters than cells
- On a page that is shown turned, a click on a text field asks for its text
  in a small editor beside the field
- A click on a menu or a list of the form shows what it offers, with what it
  holds marked. A click on a row chooses it, and so do the arrow keys and
  Enter. Escape leaves the field as it is
- A menu that takes a choice of one's own has a line to type it above its
  options. Enter or a click elsewhere keeps what was typed
- A list that takes several choices has a tick for each row. Space or a click
  ticks a row, and what is ticked is chosen when the list closes
- A field whose text the file holds but leaves to the reader to draw shows
  that text
- The pointer shows what can be filled in: an I-beam over text fields, a hand
  over boxes, buttons, menus and lists. Read-only fields are left alone
- The field clicked last has the keyboard, shown by a frame. Tab moves to the
  next field and Shift+Tab to the one before, in the order of the form (by
  rows or columns where a page asks for that), round from the last to the
  first, and the view follows. Space or Enter ticks a
  box, chooses a button, shows what a menu offers, or starts typing. Escape,
  or a click elsewhere, lets go of the form
- Highlight Form Fields (the main menu) tints every field that can be filled
  in
- Reset Form (the main menu) sets every field back to what the form has for
  it at first, which is empty for most, as one step to undo
- Filling in is undone and redone like other edits, and counts as a change to
  save
- Saved as standard form values with a new look for each field, which other
  programs show and can go on filling in
- Thumbnails and prints show what was filled in
- Not yet: text outside Latin, the forms of inserted files

## Saving

- Save (*Primary*+S) and Save As (Shift+*Primary*+S), in the main menu
- Export Selected Pages (main menu): saves the pages selected in the sidebar
  (or the current page) as a new PDF, suggesting a name like "Report (pages
  2-4, 7).pdf". The document stays as it was
- The new file is written next to the old one and then takes its place, so the
  old file is intact until the new one is complete
- Encrypted documents stay encrypted, with the same password
- A save that only adds markup or fills in empty form fields appends to the
  file instead of writing it anew. That is faster for large files and keeps a
  digital signature valid. A save that deletes or changes something writes the
  file anew, so nothing deleted stays behind in it
- Saved files keep the outline, links, metadata and page labels. After
  deleting, moving or inserting pages, each page keeps its label in the saved
  file too
- Closing a window (*Primary*+W) or quitting with unsaved changes asks "Save
  Changes?": Save, Discard or Cancel
- Edits made while a save is running stay "Edited"
- When another program changes the open file and there are no edits, it's
  reloaded in place, at the same page and zoom. With edits, a banner says so
  and offers Reload (which discards the edits), and saving over the changed
  file asks first ("File Changed on Disk"). Specto's own saves don't count as
  changes
- When the open file is moved or deleted, a banner says so, and Save writes it
  again
- A file Specto can't save correctly (a few damaged ones) isn't written, and a
  message says so instead

## Printing

- Print (*Primary*+P, the main menu) with the system's print dialog: printer,
  copies, paper, all pages, the current page, a range, or the pages selected in
  the sidebar ("Selection")
- Prints the document as edited: pages in their order, with their rotation,
  including inserted pages
- Each page is fitted to the paper and centered, and pages wider than tall go on
  landscape paper
- Pages are printed as images at 300 dpi. That is sharp on paper, but a "Print
  to File" PDF is large and has no selectable text. Export Selected Pages keeps
  the text
- The dialog remembers the last printer and settings while the app runs

## macOS

- A menu bar in the system menu bar (File, Edit, View, Go, Tools, Help) with
  every action
- Shortcuts with Cmd

## Properties

- Properties (Alt+Enter, Cmd+I on macOS, the main menu) shows what is known
  about the file: its name and folder, what it says about itself (title,
  author, subject, keywords, the programs it was made with, when it was created
  and changed), the number of pages and their size, the file size and the PDF
  version
- What the file doesn't state is left out
- Dates are on the reader's clock, and page sizes carry the name of the paper
  where it has one, such as "210 × 297 mm (A4)"
- Protection: whether the file is encrypted, whether it needs a password to
  open, and what it asks not to be done with it (printing, copying, changing
  the pages, marking up)
- The values can be selected and copied

## About

- About Specto (the main menu): the version, the license, and links to the
  project page and to where problems are reported

## Remembering state

- Per file: page and position, layout, zoom, sidebar visibility and tab
- Restored when the file is opened again. The last 200 files are kept
- Quitting (*Primary*+Q) closes all windows, so each one saves

## Under the hood

- One shared pool of render threads for all windows
- Protection against pathological pages: slow pages render one tile at a time,
  and extra workers start when renders stall

## Not yet

- Highlighting the current outline entry
- Dark mode for page content
