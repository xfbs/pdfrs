# Android app features

What the Android app currently does. Update this file in the same commit as any
change to a feature. For testing these, see [TESTING.md](TESTING.md). What is
planned but not built is in [docs/TASKS.md](../../docs/TASKS.md).

The menu behind the three dots in the top bar is called the overflow menu (⋮)
here.

## Opening files

- Start screen with an "Open PDF" button and, once documents have been opened,
  a list of recent ones
- Open with the system's file picker, limited to PDFs, or from another app
  ("Open with"). A document opened from another app replaces the one that is
  open
- Recent documents: up to 20, newest first, each with the page it was left on
  and when. Files handed over by another app are usually not listed, because
  the app isn't allowed to read them again later
- A recent file that has been moved or deleted shows "Couldn't open this file"
  and disappears from the list
- A document opens where it was left, with the same page, position, layout and
  zoom. It is recognized by its content, so a copy or a renamed file continues
  too
- Password dialog for protected files, with a retry on a wrong password
- Files that aren't valid PDFs show "Couldn't open this file" and offer to open
  another
- Large files (64 MB and up) are not loaded into memory but read piece by piece
  as pages are shown, so they open about as quickly as small ones
- Close in the overflow menu returns to the start screen

## The top bar

- On the left, a button for Pages and Contents. On the right, search, the view
  button and the overflow menu
- The view button's icon pictures the current layout. Its menu has the layouts
  (Single page and Continuous, and on wide windows also Two pages and Two
  pages, continuous), then Fit page and Fit width. The current choices are
  ticked, and everything in this menu is remembered per document
- The overflow menu has Open, Share, Print, Save as, Go to page, Properties,
  Settings and Close. After an edit it also has Save (where the file can be
  written over) and Undo, and after undoing, Redo
- On the start screen the overflow menu has only Open and Settings
- Properties shows the file name, what the document says about itself (title,
  author, subject, keywords, the programs it was made with, when it was created
  and changed), the number of pages, their size with its name (in millimeters,
  such as A4, or in inches for sizes like Letter), the file size and the PDF
  version. Values can be selected and copied

## Reading one page at a time

This is the default layout.

- Every page is shown whole and as large as fits, whatever its size or
  orientation
- A swipe left or right turns the page. The page follows the finger with the
  next one beside it, and turns past half way or with a flick. Otherwise it
  slides back
- One swipe turns exactly one page, however fast
- Pinch zooms around the fingers. A page can't get smaller than fitted
- Zoomed in, a drag moves within the page, and a flick stops at the page edge
  without turning the page
- Dragging further at the edge of a zoomed page turns it, and the next page
  slides in fitted. Going to another page any other way also returns to the
  fitted view
- A double tap zooms to 2.5 times around the tap, and a second one back to
  fitted
- Pages sharpen about a quarter of a second after zooming stops

## Continuous scrolling

- Continuous in the view menu shows one column of pages fitted to the width,
  on the same page as before
- Scrolling by dragging and flicking
- Pinch and double tap work as in the paged layout. A double tap returns to fit
  width
- New documents open one page at a time. A document opened before opens in the
  layout it was left in

## Fit page and fit width

- Fit width makes pages fill the width, in either layout. One page at a time, a
  page taller than the screen is dragged to read down it, and a swipe sideways
  turns it. The next page starts at its top
- Fit page shows every page whole. With Continuous, whole pages scroll past
- Picking the ticked fit again undoes any zooming, and so does a double tap
- Each layout starts with its own fit: single pages whole, continuous scrolling
  fitted to the width. A different choice lasts until the layout is switched
- The fit is remembered per document

## Navigation

- A page indicator ("n / total") above the thumbnail strip
- A strip of page thumbnails along the bottom, with the current page outlined
  and kept in the middle. Tapping a thumbnail goes there, and scrolling the
  strip flips through the document. One-page documents have no strip
- Pages, from the top-left button: a grid of all pages with the current one
  outlined
- Contents, a second tab for documents with an outline: a collapsible outline
  with page numbers and the current section highlighted. Long outlines start
  collapsed to their top level
- The two tabs can be swiped, and the one used last is shown again
- Go to page in the overflow menu, with a field that shows the valid range
- Printed page numbers: the indicator shows them with the position ("iii (3 /
  240)"), Pages and Contents show them, and Go to page accepts a printed number
  first and a position otherwise
- Links inside the document jump to their target. Web and email links show
  their address and open only after confirmation. Other kinds say they can't be
  opened

## Search

- The magnifying glass turns the top bar into a search field
- Matches are highlighted in yellow as you type, the current one in orange, and
  the view goes to the first match from the current page on
- A count shows which match is current and how many there are ("3 of 17"), or
  "No matches"
- The arrows beside the count go to the next and previous match, across pages,
  wrapping around at the ends
- Case and accents don't matter, and neither does a line break or a hyphen at a
  line's end inside the search text
- The first search reads the whole document in the background, so the count
  grows for a moment. Later searches are answered at once
- The keyboard's search key, or a tap on the page, puts the keyboard away. The
  search and its highlights stay, and the arrows still work
- The arrow left of the field, or Back, leaves the search. The highlights go
  and the view stays where it is

## Text

- A long press selects a word, with a handle at each end and the system's
  floating menu beside it: Highlight, Copy, Share, Underline, Strike through
  and Select all. Where they don't all fit, the last ones are behind the
  menu's three dots
- Moving on after the long press, without lifting the finger, selects further
  words
- The handles move each end letter by letter, also on to other lines
- The selection stays with its text through scrolling, zooming, page turns and
  rotation
- In continuous scrolling, dragging to the top or bottom edge scrolls on, and
  the selection grows over as many pages as you like
- Copy puts the text on the clipboard, with lines and pages separated by line
  breaks
- Share hands the text to another app through the share sheet, up to about
  100,000 characters
- Select all selects the whole document
- A tap on the page, or Back, lets go of the selection

## Marking text

Marks are annotations, as other PDF programs make and show them. Like page
edits, they change nothing in a file until the document is saved.

- Highlight, in the menu beside selected text, marks it in yellow, as with a
  highlighter: the text stays readable under it
- Underline and Strike through draw a red line under or through it
- A selection over several lines or pages is marked as a whole
- A tap on a mark picks it: a box is drawn around it, and the menu offers
  Delete. A tap elsewhere, or Back, lets go of it
- Marks that were in the file already, and other annotations in it (notes,
  drawings, text boxes), can be picked and deleted the same way
- Where a mark lies on a link, a tap follows the link
- Marking and deleting are edits: "Edited" shows beside the title, Undo and
  Redo in the overflow menu take them back and repeat them ("Undo add
  annotation"), and they are saved, shared and printed with the document
- The thumbnails show marks as the page does, in the strip, the page grid and
  the sidebar

## Forms

- A tap ticks or unticks a checkbox, and chooses a radio button
- A tap on a text field asks for its text in a dialog, under the name the form
  has for the field. Done, or the keyboard's own button, fills it in. Cancel
  leaves the field as it is
- A field of several lines takes several in the dialog. A password field hides
  what is typed. A field that takes only so many characters takes no more, and
  the dialog counts them
- A tap on a menu or a list of the form shows what it offers, with what it
  holds marked. A tap on a row chooses it
- A menu that takes text of one's own has a line to type it above its
  options. Done fills it in
- A list that takes several choices has a tick for each row. Done fills in
  the ticked ones
- Highlight form fields (the overflow menu) tints every field that can be
  filled in, until it is chosen again. The menu shows a tick while it is on
- Reset form (the overflow menu) sets every field back to what the form has
  for it at first, as one edit ("Undo reset form")
- Both are in the menu only for a document with a form to fill in
- A box is drawn around the field while its dialog is open
- Read-only fields are left alone
- Where a field lies on a link, a tap follows the link. Where a mark lies on a
  field, a tap fills in the field
- Filling in is an edit: "Edited" shows beside the title, Undo and Redo in the
  overflow menu take it back and repeat it ("Undo fill in field"), and it is
  saved, shared and printed with the document
- The thumbnails show what was filled in
- Not yet: typing on the page itself, moving from field to field with the
  keyboard, text outside Latin

## Editing pages

Pages can be rotated, deleted and reordered. Nothing changes a file until the
document is saved.

- A long press on a thumbnail in the page grid, or in the tablet's sidebar,
  picks the page. The top bar then counts the picked pages ("1 selected") and
  offers what can be done with them, and a tap picks further pages or lets go
  of them
- The close button in that bar, or Back, lets go of all of them
- Select all in the overflow menu picks every page
- Rotate left and right turn the picked pages by a quarter. They stay picked,
  to be turned further
- Delete removes the picked pages. The pages after them keep their printed
  numbers. A document keeps at least one page
- A long press that moves on drags the picked pages to another place. A bar
  between two thumbnails shows where they would go, and the grid scrolls near
  its top and bottom edge
- Undo and Redo in the overflow menu name the edit ("Undo delete") and take
  back or repeat one edit at a time
- "Edited" beside the title while there are unsaved edits. Undoing back to the
  document as opened or saved removes it

## Saving

- Save in the overflow menu, while there are unsaved edits, writes over the
  file the document was opened from. The new PDF is written to the app's own
  storage first and then copied over the file, so a document that can't be
  written leaves the file as it was. The document stays open, and the edits can
  still be undone
- Save is only offered where the app may write to the file, which is the case
  for documents opened with the file picker. Documents handed over by other
  apps mostly come read-only
- Save as is always there. The system asks where and under what name, the
  document as edited is written there as a new PDF, and that file is then the
  one that is open, at the same place
- Leaving a document with unsaved edits asks "Save changes?", with Save (or
  Save as), Discard and Cancel
- Unsaved edits outlast the app. They are kept on the device whenever the app
  goes into the background, and if Android ends it meanwhile, the document is
  open again with its edits when the app comes back. The same happens when the
  document is next opened in any other way. There is nothing to undo then
- Discard changes in the overflow menu, while there are unsaved edits, goes
  back to the file as it is, after asking
- Unsaved edits that weren't looked at for 30 days are deleted. So are those of
  a file that changed, and a document with a password keeps none

## Sharing

- Share in the overflow menu offers the open document through the system's
  share sheet. The receiving app gets the whole file under its name, and a
  protected file stays protected
- With unsaved edits, the document as edited is shared, and sharing saves
  nothing
- Not offered on the start screen, or after a file failed to open

## Printing

- Print in the overflow menu opens the system's print dialog, with a preview of
  every page. Printer, copies, paper and page ranges are the system's to offer,
  and it can also save the pages as a PDF
- The pages are handed over as they are, not as pictures, so text stays text
- The document is printed as edited, without saving it
- The dialog starts with landscape paper when the first page is wider than tall
- For a protected document, the print service gets a copy without the password,
  which is deleted when the dialog closes. The file itself stays protected

## Full screen

- A tap on the page, where there is no link, hides the top bar, the strip, the
  page indicator and the status bar. The navigation handle stays
- The page uses the entire screen, including the area around the camera cutout
- Turning a page shows the page number briefly at the bottom
- A tap or Back brings the bars back
- Swiping, zooming and links work as in the normal view

## Tablets and other wide windows

These apply when the app's window is at least 840 dp wide, such as a tablet in
landscape. Narrower windows, such as a tablet in portrait or in split screen,
get the phone layout.

- Pages and Contents are in a sidebar on the left, open from the start, with
  the document beside it. The page list is one column and scrolls to keep the
  current page in view
- The top-left button folds the sidebar away. Whether it was open is remembered
  per document
- The thumbnail strip is hidden while the sidebar is open
- Full screen hides the sidebar along with the bars
- Two pages and Two pages, continuous in the view menu show pages in pairs,
  fitted together. The page indicator names both, and a swipe turns a pair
- First page alone makes the first page stand by itself, so that a book's left
  and right pages land left and right
- While the window is narrow, a document set to two pages is shown one page at
  a time

## Keyboard

With a keyboard attached. The page takes the keyboard focus when a document
opens.

- Right and Left, Page Down and Page Up, Space and Shift+Space, and Backspace
  turn the page, or the pair in a two-page layout
- In continuous scrolling, Right and Left go to the top of the next or previous
  page, and Page Down, Page Up and Space scroll most of a screenful
- On a page taller than the view, Page Down and Space scroll down it first and
  turn the page at its end
- Down and Up scroll a little. One page at a time, they turn the page where it
  has no more to show in that direction
- Home and End go to the first and last page
- Ctrl with Plus and Minus zooms, and Ctrl+0 returns to the fitted view
- Ctrl+A selects all text, Ctrl+C copies the selection, Escape lets go of it
- Ctrl+Z undoes, and Shift+Ctrl+Z or Ctrl+Y redoes
- Ctrl+S saves over the file where that is possible, and otherwise asks where
  to save
- Ctrl+P prints
- Ctrl+F opens the search. In the search field, Enter and Shift+Enter go to the
  next and previous match, and Escape leaves

## Settings

Settings in the overflow menu, with or without a document open. They hold for
the whole app and are kept across launches.

- Keep the screen on, off by default. The screen stays on for ten minutes after
  the last touch or key while a document is open
- Volume keys turn pages, off by default. Volume down goes forward and volume
  up goes back, like Page Down and Page Up, and holding a key keeps going. This
  only applies in the page view. Everywhere else the keys change the volume
- Crash log: errors that ended the app, and errors inside the PDF library, with
  the app's version and the device. It can be shared, for a bug report, or
  deleted. It stays on the device unless shared, and only its newest entries
  are kept. A crash outside the app's own code, such as running out of memory,
  leaves no entry

## General

- In more than 30 languages, following the device's language. On Android 13 and
  newer, the language can also be chosen for Specto alone in the system
  settings. Dates and numbers follow the language. Still in English: the reason
  given when a save or open fails, and the crash log
- Rotating the device keeps the document and the page, and nothing reloads
- In landscape the page view extends beside the camera cutout, while the top
  bar, Pages, Contents and dialogs keep clear of it
- Bars, backgrounds and dialogs follow the system's light and dark mode, with
  the system's colors on Android 12 and newer. Pages stay as the document
  defines them
- Pages appear blurry first and then sharp, and the app stays responsive in
  documents of several hundred pages
- A spinner over pages that take long to render (more than 100 megapixels of
  images, or a great many drawing operations) while they aren't sharp yet

## Not yet

- Inserting pages from other files
- Other annotations (notes, drawings, text boxes), and choosing a mark's color
