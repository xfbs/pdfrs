# Changelog

What changed in each release of Specto, for the people who use it. Both apps
share one version. A release's section is its notes on GitHub.

## Unreleased

### Android

- Edits made after Save, and kept while Android ended the app, come back as
  they were. The edits from before saving appear once, not twice.
- A document that couldn't be saved where you chose, and is shown from the
  app's own copy instead, can be saved again without being emptied.
- Going back while a large document opens leaves it on the start screen, and
  the documents listed there can all be opened.

## 0.2 (2026-11-18)

### Both apps

- Specto is translated into more than 30 languages, on
  [Crowdin](https://crowdin.com/project/specto), and the desktop app is
  translated now too. Apart from German and Dutch, most translations were made
  by machine and still need checking by people who speak the language.
- Saving is several times faster for large files when it only adds markup or
  fills in empty form fields: the file is kept as it is, with the changes
  added at its end.

### Desktop

- Trim Margins, in the page layout menu, shows each page without its empty
  margins, so it fills more of the window. It works in the single and two page
  layouts, and is remembered for each file.
- Back and Forward return to where you were before following a link, a
  Contents entry or a search result, or going to a page by its number. Use
  Alt+Left and Alt+Right (Cmd+[ and Cmd+] on macOS) or the back and forward
  buttons of a mouse.
- Resting the pointer on a link to another place in the document shows that
  place in a small panel, so a footnote can be read without going there.
- Properties (Alt+Enter, Cmd+I on macOS) shows what is known about the file:
  its title and author, the programs it was made with, its pages and paper
  size, and how it is protected.

### Android

- Unsaved edits are no longer lost when Android ends the app in the
  background. The document comes back with its edits, also when it is opened
  again later.
- Discard changes in the overflow menu goes back to the file as it is.

## 0.1 (2026-11-15)

The first release, of the desktop app for Linux and the Android app.

### Both apps

- Reading: sharp pages at any zoom, single and two page layouts and continuous
  scrolling, thumbnails, the table of contents, links and printed page
  numbers. Encrypted files open, and each document opens where it was left.
- Text: selecting, copying and searching, and highlighting, underlining and
  striking it through.
- Pages: rotating, deleting and reordering them.
- Forms: checkboxes, radio buttons, text fields, menus and lists.
- Printing, and saving over the file or as a new one.

### Desktop

- Inserting pages from other PDFs, and exporting a selection of pages.
- Drawing, shapes, text boxes, notes and signatures.
- English only.

### Android

- Sharing a document with other apps, and the document's properties.
- English and German.
