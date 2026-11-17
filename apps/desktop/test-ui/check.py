"""Checks the desktop app end to end in a headless session.

    cargo build -p specto-desktop
    apps/desktop/test-ui/run.sh python3 -I apps/desktop/test-ui/check.py [BINARY] [TEST]...
    SPECTO_SCALE=2 apps/desktop/test-ui/run.sh python3 -I apps/desktop/test-ui/check.py

Starts specto-gtk with SPECTO_DEBUG=1 and drives it over D-Bus with its window
actions plus the test hooks in apps/desktop/src/debug.rs: `debug-state`
dumps the window's state as JSON, `debug-screenshot` saves a PPM image.

The test PDFs are generated here. Each page is filled with its own color, so
a screenshot shows which page is on screen and that it was rendered.

Tests (run some by naming them, such as `zoom layouts`):

- start_page: no file opens the start page
- about: the About dialog, with the version
- properties: the Properties dialog says what the file is: its name and
  folder, the pages and their size, the file size and how it is protected
- translated: started in Dutch, the app shows its texts in Dutch
- open_and_render: title, page labels in the entry, default layout, zoom and
  sidebar. The screenshot is at device resolution and shows page 1
- navigation: next, previous, first and last page. A rotated page renders
- page_entry: labels win over numbers ("4" is the page labeled 4), case
  doesn't matter, unknown pages are refused
- layouts: all four keep the current page. Two-page shows two pages. Cover
  page alone
- zoom: actual size, in, out, fit page (the whole page visible), limits
  5% and 3200% and rendering there
- sidebar: hiding it, the Contents tab
- remembers_state: page, layout, zoom and sidebar survive closing the app
- broken_file: an error message, and the app keeps running
- two_windows: one window per file, navigated independently
- rotate_and_undo: Ctrl+Right and Ctrl+Left turn the current page, both
  pages of a spread in the two-page layout. "Edited" in the title. Ctrl+Z
  and Shift+Ctrl+Z. In the page entry, Ctrl+Left moves the cursor instead
- save: a rotated page is saved (opened again, it's still rotated, and
  the page labels are kept). Save As writes a copy and shows it, leaving
  the original as it was, and the copy opens where the original was left
- close_prompt: quitting with unsaved changes asks first. Cancel keeps the
  window, Discard closes without saving, Save saves and closes
- thumbnail_selection: clicking a thumbnail goes to its page. Shift+click
  and Ctrl+click select several, which stay selected while the view moves
  and are what Rotate turns. Ctrl+A in the sidebar selects every page
- delete_pages: Delete and Backspace on the document delete no page
  (Backspace scrolls back). In the sidebar they delete the selected pages,
  or the current one. Not in the page entry. Never the last page. Undo
  brings them back
- move_pages: thumbnails dropped (through a test hook) on the lower or
  upper half of another go after or before it. The view stays on the page
  it showed. A drop in place does nothing
- drop_files_and_export: two PDFs dropped (through a test hook) on the
  sidebar are inserted where dropped, as one edit. Export Selected Pages
  writes those pages as a new file and leaves the document as it was
- pages_between_windows: pages of one window dropped (through a test
  hook) on another's thumbnails are copied there, rotation included. The
  first window keeps them. Saved, they're in the second file
- file_changed: the file replaced by another program is reloaded in place
  if there are no edits. With edits, a banner offers to, and Save asks
  before overwriting. Our own saves don't count. A deleted file gets a
  banner
- print_to_file: printing (to a PDF file, through a test hook) prints the
  pages as edited, landscape ones on landscape paper, each with its own
  content. "Selection" prints the selected thumbnails only
- insert_file: pages of another file inserted after the current page, shown
  and searched. The pages keep their labels, the inserted ones numbered by
  position. Undo. Saved, they're in the file

With the virtual pointer and keyboard, on a Courier text PDF:

- link_click: clicking a link follows it, Alt+Left goes back to where it
  was clicked and Alt+Right forward again
- markup_bar: with a tool of the markup bar pressed in, text is marked as
  it is selected, in the tool's color. A color chosen while an annotation
  is picked recolors it. Closing the bar lets go of the tool
- draw_tools: the pen, rectangle, ellipse and line tools draw where the
  pointer is dragged, also over text, each in its color. A click draws no
  shape. A line is picked near it only. A drag moves a shape and a handle
  resizes it, Escape gives that up. Saved, they're in the file and shown,
  and move as before
- text_tools: the note tool puts a note where the page is clicked and asks
  for its text. The text tool asks for text and makes a text box of it. A
  double click or Enter asks again. A text box moves with its text. Saved,
  both are in the file, and a text box emptied is deleted
- markup_list: the Markup page of the sidebar lists every annotation with
  its page, kind and text, follows edits, and a row picks its annotation and
  shows its page. Backspace in the list deletes the marked row's annotation
  and never pages. It is filled from the file when a document opens on it
- signatures: a saved signature, drawn or from a photo, is placed by one
  click, as a drawing or a picture. The picture moves and keeps its
  proportions when resized. Saved, both are in the file, and the picture
  still moves. The dialog for a new signature opens
- text_style: the size and font controls show with the text tool and a
  picked text box. What is typed is set as they say. A picked box takes
  what is set then, and shows its own. Saved, it is what it was
- typed_text_is_kept: text being typed is in the document when it is saved
  or the window closed, goes to the box it was typed into whatever is
  clicked meanwhile, also from the size field, and before pages are inserted
- fill_form: on a form, the pointer shows what can be filled in; a click
  ticks a box and chooses a radio button, and a text field is typed into
  where it is, Enter ending it; Tab and Shift+Tab go through the fields,
  Space and Enter tick and choose; a menu lists what it offers, and the
  arrow keys and Enter choose; a password field hides what is typed; on a
  turned page the text is asked for beside the field; undo and redo; text
  being typed is saved;
  saved, the values are in the file and shown
- fill_form_comb: a comb field's letters stand in the middle of their cells
  while typing, as they do afterwards, and it takes no more than it has
  cells
- fill_form_choices: a menu that takes text of one's own has a line to type
  it, Enter keeps it and Escape drops it, the arrow keys go on to the
  options, and a save keeps what is being typed; a list that takes several
  has a tick for each row, and what is ticked is chosen when it closes;
  Highlight Form Fields tints the fields, and Reset Form empties them in
  one step; saved, the values are in the file
- mark_text: Highlight and Underline mark the selected text (and say so
  when there is none) and are printed. A click picks an annotation, Delete deletes it and
  not the page, Escape lets go. Undo. Saved, they're in the file and shown.
  one the file has is picked and deleted the same way, and the page is
  then shown without it
- back_and_forward: Back and Forward after going to the last page or to a
  page by its number, not after turning a page. A jump after going back
  drops what was ahead
- select_by_dragging: the I-beam cursor over text. Dragging selects,
  forwards and backwards over lines. Escape and a click clear it
- select_words_and_lines: double-click selects a word, triple-click a line,
  Shift+click extends by lines, double-click and drag extends by words
- drag_on_link_selects: a drag starting on a link selects instead
- copy_and_select_all: Ctrl+C puts the selection on the clipboard (read back
  through the compositor). Ctrl+A selects every page
- select_across_pages: holding a drag at the bottom edge scrolls, and the
  selection continues onto the next page
- select_rotated_page: selecting on a page rotated by 90 degrees
- select_after_rotating: selecting and searching on a page the user rotated
- search: Ctrl+F and typing. Case, a line break, a hyphenated word and
  accents don't stop a match. Enter, Shift+Enter and wrapping around. The
  view follows. "No matches". Escape ends the search
- search_indexes_in_background: the first match after the current page,
  found while the pages are indexed in the background

Saved state and recent files go to a temporary directory, not the user's.
Screenshots of failed tests are kept in $SPECTO_CHECK_OUT (default: a
temporary directory, printed at the end).

Standard library only, except for the pointer, keyboard and clipboard
tests, which use mutter's remote desktop API through PyGObject.
"""

import json
import re
import os
import shutil
import subprocess
import sys
import tempfile
import time
import traceback

APP_ID = "net.xfbs.specto"
APP_PATH = "/net/xfbs/specto"
TIMEOUT = float(os.environ.get("SPECTO_CHECK_TIMEOUT", "20"))

# One color per page: the corners of the RGB cube at 0, 128 and 255,
# without grays (the UI's colors) and blues close to the accent color.
PALETTE = [
    (r, g, b)
    for r in (0, 128, 255)
    for g in (0, 128, 255)
    for b in (0, 128, 255)
    if not (r == g == b) and (r, g, b) not in ((0, 128, 255), (0, 0, 255), (128, 128, 255))
]

PAGES = 12
# Labels: i, ii, iii, then 1 to 9.
LABELS = ["i", "ii", "iii"] + [str(n) for n in range(1, PAGES - 2)]
ROTATED = 5


# Test PDFs


def write_pdf(path, objects):
    """Writes `objects` (bodies of objects 1 to n, object 1 the catalog)."""
    out = bytearray(b"%PDF-1.7\n")
    offsets = []
    for i, body in enumerate(objects):
        offsets.append(len(out))
        out += f"{i + 1} 0 obj\n".encode() + body + b"\nendobj\n"
    xref = len(out)
    out += f"xref\n0 {len(objects) + 1}\n0000000000 65535 f \n".encode()
    for o in offsets:
        out += f"{o:010} 00000 n \n".encode()
    out += f"trailer << /Root 1 0 R /Size {len(objects) + 1} >>\n".encode()
    out += f"startxref\n{xref}\n%%EOF\n".encode()
    with open(path, "wb") as f:
        f.write(out)


def stream(data):
    return f"<< /Length {len(data)} >>\nstream\n".encode() + data + b"\nendstream"


def pages_pdf(path):
    """PAGES letter pages, page i filled with PALETTE[i] and titled "Page
    i+1". Page labels, an outline with two entries, and page ROTATED rotated
    by 90 degrees."""
    # 1 catalog, 2 pages, 3 font, 4 outlines, 5-6 outline items,
    # then a page and a content stream per page.
    first = 7
    page_ids = [first + 2 * i for i in range(PAGES)]
    objects = [
        b"<< /Type /Catalog /Pages 2 0 R /Outlines 4 0 R /PageMode /UseOutlines"
        b" /PageLabels << /Nums [0 << /S /r >> 3 << /S /D >>] >> >>",
        f"<< /Type /Pages /Kids [{' '.join(f'{p} 0 R' for p in page_ids)}]"
        f" /Count {PAGES} >>".encode(),
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica"
        b" /Encoding /WinAnsiEncoding >>",
        b"<< /Type /Outlines /First 5 0 R /Last 6 0 R /Count 2 >>",
        f"<< /Title (Chapter One) /Parent 4 0 R /Next 6 0 R"
        f" /Dest [{page_ids[3]} 0 R /XYZ 0 792 0] >>".encode(),
        f"<< /Title (Chapter Two) /Parent 4 0 R /Prev 5 0 R"
        f" /Dest [{page_ids[8]} 0 R /XYZ 0 400 0] >>".encode(),
    ]
    for i in range(PAGES):
        r, g, b = (c / 255 for c in PALETTE[i])
        content = (
            f"{r:.3f} {g:.3f} {b:.3f} rg 36 36 540 690 re f "
            f"0 g BT /F1 24 Tf 72 740 Td (Page {i + 1}) Tj ET"
        ).encode()
        rotate = " /Rotate 90" if i == ROTATED else ""
        objects.append(
            f"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792]{rotate}"
            f" /Resources << /Font << /F1 3 0 R >> >> /Contents {page_ids[i] + 1} 0 R >>".encode()
        )
        objects.append(stream(content))
    write_pdf(path, objects)


# Text for selection tests, in Courier: every character is 0.6 em wide, so
# positions are easy to compute. Page 3 is rotated by 90 degrees.
TEXT = [
    ["The quick brown fox", "jumps over the lazy dog", "Link to page two"],
    ["Second page", "and its last line", "Search across a line", "break and a hyphen-",
     "ated word, Café"],
    ["Rotated text"],
]
FONT, CHAR, LEFT, TOP, LEADING = 20, 12, 72, 700, 30
LINK_LINE = 2


def text_point(page, line, col, before=True):
    """PDF point inside character `col` of `line`: in its left quarter
    (selects from before it) or its right quarter (to after it), at half
    height."""
    x = LEFT + col * CHAR + (CHAR * 0.25 if before else CHAR * 0.75)
    return x, TOP - line * LEADING + FONT * 0.3


def text_pdf(path):
    """TEXT, one page per entry. A link on page 1's last line to page 2."""
    n = len(TEXT)
    page_ids = [5 + 2 * i for i in range(n)]
    link_rect = (LEFT - 2, TOP - LINK_LINE * LEADING - 6,
                 LEFT + len(TEXT[0][LINK_LINE]) * CHAR + 2, TOP - LINK_LINE * LEADING + 18)
    objects = [
        b"<< /Type /Catalog /Pages 2 0 R >>",
        f"<< /Type /Pages /Kids [{' '.join(f'{p} 0 R' for p in page_ids)}]"
        f" /Count {n} >>".encode(),
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Courier /Encoding /WinAnsiEncoding >>",
        f"<< /Type /Annot /Subtype /Link /Rect [{' '.join(map(str, link_rect))}]"
        f" /Border [0 0 0] /Dest [{page_ids[1]} 0 R /XYZ 0 792 0] >>".encode(),
    ]
    for i, lines in enumerate(TEXT):
        ops = " ".join(
            f"BT /F1 {FONT} Tf {LEFT} {TOP - k * LEADING} Td ({t}) Tj ET"
            for k, t in enumerate(lines)
        )
        extra = " /Annots [4 0 R]" if i == 0 else ""
        extra += " /Rotate 90" if i == 2 else ""
        objects.append(
            f"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792]{extra}"
            f" /Resources << /Font << /F1 3 0 R >> >> /Contents {page_ids[i] + 1} 0 R >>".encode()
        )
        # WinAnsi: "é" is one byte.
        objects.append(stream(ops.encode("cp1252")))
    write_pdf(path, objects)


# Screenshots


def read_ppm(path):
    """Returns (width, height, pixel bytes as RGB) of a binary PPM."""
    with open(path, "rb") as f:
        data = f.read()
    magic, size, maxval, pixels = data.split(b"\n", 3)
    assert magic == b"P6" and maxval == b"255", "not an 8-bit binary PPM"
    width, height = map(int, size.split())
    return width, height, pixels


def page_colours(path, step=4):
    """Fraction of (sampled) pixels showing each page's color: {page: share}."""
    width, height, pixels = read_ppm(path)
    colours = {c: i for i, c in enumerate(PALETTE[:PAGES])}
    counts, total = {}, 0
    for y in range(0, height, step):
        row = pixels[y * width * 3 : (y + 1) * width * 3]
        for x in range(0, width * 3, step * 3):
            total += 1
            i = colours.get(tuple(row[x : x + 3]))
            if i is not None:
                counts[i] = counts.get(i, 0) + 1
    return {i: n / total for i, n in counts.items()}


# Driving the app


class CheckFailed(Exception):
    pass


def check(cond, message):
    if not cond:
        raise CheckFailed(message)


def gdbus_activate(path, action, param=None):
    arg = "[]" if param is None else f"[{param}]"
    r = subprocess.run(
        ["gdbus", "call", "--session", "--dest", APP_ID, "--object-path", path,
         "--method", "org.gtk.Actions.Activate", action, arg, "{}"],
        capture_output=True, text=True, timeout=10,
    )
    if r.returncode != 0:
        raise CheckFailed(f"{action} on {path}: {r.stderr.strip()}")


def gvariant_str(s):
    return "<'" + s.replace("\\", "\\\\").replace("'", "\\'") + "'>"


class App:
    """A running specto-gtk."""

    def __init__(self, ctx, files=()):
        self.ctx = ctx
        log = open(os.path.join(ctx.tmp, "app.log"), "a")
        self.proc = subprocess.Popen(
            [ctx.binary, *files], env=ctx.env, stdout=log, stderr=log
        )
        deadline = time.monotonic() + TIMEOUT
        while True:
            r = subprocess.run(
                ["gdbus", "introspect", "--session", "--dest", APP_ID,
                 "--object-path", f"{APP_PATH}/window/1"],
                capture_output=True, timeout=10,
            )
            if r.returncode == 0 and b"org.gtk.Actions" in r.stdout:
                return
            if self.proc.poll() is not None:
                raise CheckFailed(f"app exited with {self.proc.returncode}")
            if time.monotonic() > deadline:
                raise CheckFailed("app didn't start")
            time.sleep(0.1)

    def window(self, n=1):
        return Window(self, n)

    def quit(self):
        gdbus_activate(APP_PATH, "quit")
        try:
            self.proc.wait(timeout=TIMEOUT)
        except subprocess.TimeoutExpired:
            raise CheckFailed("app didn't quit")
        check(self.proc.returncode == 0, f"app exited with {self.proc.returncode}")

    def kill(self):
        if self.proc.poll() is None:
            self.proc.kill()
            self.proc.wait()


class Input:
    """A virtual pointer and keyboard, and the clipboard, through mutter's
    remote desktop D-Bus API (in the headless session only). Coordinates are
    logical screen pixels; with the window maximized they are window
    coordinates. Needs PyGObject."""

    BUTTON_LEFT = 0x110  # BTN_LEFT
    # Keys as Linux input key codes (linux/input-event-codes.h), which
    # don't depend on the keyboard layout.
    ESCAPE, CONTROL, SHIFT, ENTER, KEY_A, KEY_C, KEY_F = 1, 29, 42, 28, 30, 46, 33
    LEFT, RIGHT, KEY_Z, ALT = 105, 106, 44, 56
    BACKSPACE, DELETE, TAB, SPACE, DOWN = 14, 111, 15, 57, 108
    # Key codes for typing, on a US layout (mutter's default).
    TYPING = {
        **{c: 16 + i for i, c in enumerate("qwertyuiop")},
        **{c: 30 + i for i, c in enumerate("asdfghjkl")},
        **{c: 44 + i for i, c in enumerate("zxcvbnm")},
        **{c: 2 + i for i, c in enumerate("1234567890")},
        " ": 57, "-": 12, ",": 51, ".": 52,
    }
    # GTK counts clicks within this many seconds as one sequence (double-
    # and triple-clicks). Wait this long before clicking somewhere new.
    CLICK_GAP = 0.6

    def __init__(self):
        from gi.repository import Gio, GLib

        self.GLib = GLib
        self.bus = Gio.bus_get_sync(Gio.BusType.SESSION)
        self.dest = "org.gnome.Mutter.RemoteDesktop"
        self.session = self._call(
            "/org/gnome/Mutter/RemoteDesktop", self.dest, "CreateSession"
        )[0]
        self.iface = self.dest + ".Session"
        self._session("Start")
        self._session("EnableClipboard", "(a{sv})", ({},))
        # Relative motion only (absolute needs a screen cast): park the
        # pointer in the top-left corner, then track where it is.
        self.last_press = 0.0
        self.window = None
        self.screen = None
        self.park()
        # Mutter creates the virtual keyboard on its first event, and that
        # event is lost: send a harmless one first.
        self.keys(self.SHIFT)
        time.sleep(0.2)

    def _call(self, path, iface, method, sig=None, args=None):
        params = self.GLib.Variant(sig, args) if sig else None
        r = self.bus.call_sync(self.dest, path, iface, method, params, None, 0, 5000, None)
        return r.unpack() if r else None

    def _session(self, method, sig=None, args=None):
        return self._call(self.session, self.iface, method, sig, args)

    def park(self):
        """Moves the pointer to the top-left corner, where it stops."""
        # Twice: the first event of a new device can be lost (see below).
        for _ in range(2):
            self._session("NotifyPointerMotionRelative", "(dd)", (-1e5, -1e5))
        self.pos = (0.0, 0.0)

    def move(self, x, y):
        """Moves the pointer to window (x, y). With a window `attach`ed and
        the target on its canvas, also makes sure it got there: relative
        motion is dead reckoning, and a single event that's lost or handled
        late (it happens, more under load) shifts every later position.
        So it reads where the canvas saw the pointer and corrects."""
        self._relative(x - self.pos[0], y - self.pos[1])
        if self.screen:
            # The pointer stops at the screen's edges.
            x = min(max(x, 0.0), self.screen[0] - 1)
            y = min(max(y, 0.0), self.screen[1] - 1)
        self.pos = (x, y)
        if self.window is None or not self.window.on_canvas(x, y):
            return
        for _ in range(8):
            try:
                self.window.pointer_at(x, y, timeout=0.5)
                return
            except CheckFailed:
                s = self.window.state()
                seen = (s["pointer"][0] + s["canvas"][0], s["pointer"][1] + s["canvas"][1])
                # Correct by the difference. A stale report gets a fresh
                # one this way too, and the next round corrects that.
                self._relative(x - seen[0], y - seen[1])
        self.window.pointer_at(x, y)

    def _relative(self, dx, dy):
        if abs(dx) < 0.5 and abs(dy) < 0.5:
            # No motion would mean no event to see: a detour.
            self._session("NotifyPointerMotionRelative", "(dd)", (1.0, 0.0))
            dx -= 1.0
        self._session("NotifyPointerMotionRelative", "(dd)", (dx, dy))

    def attach(self, window):
        """Waits for `window` to see each pointer move (see `move`). The
        window is maximized, so its size is the screen's."""
        s = window.state()
        self.window, self.screen = window, (s["width"], s["height"])
        self.park()
        return self

    def button(self, down):
        self._session("NotifyPointerButton", "(ib)", (self.BUTTON_LEFT, down))
        if down:
            self.last_press = time.monotonic()

    def new_sequence(self):
        """Waits until the next press starts a new click sequence."""
        wait = self.last_press + self.CLICK_GAP - time.monotonic()
        if wait > 0:
            time.sleep(wait)

    def click(self, x, y, times=1, shift=False, control=False):
        self.new_sequence()
        self.move(x, y)
        held = [k for k, on in ((self.SHIFT, shift), (self.CONTROL, control)) if on]
        for k in held:
            self.key_down(k)
        for _ in range(times):
            self.button(True)
            self.button(False)
        for k in reversed(held):
            self.key_up(k)

    def drag(self, start, end, steps=10, release=True, press=True):
        """Presses at `start` (unless the button is already down there) and
        moves to `end` in steps."""
        if press:
            self.new_sequence()
            self.move(*start)
            self.button(True)
        for i in range(1, steps + 1):
            t = i / steps
            self.move(start[0] + (end[0] - start[0]) * t, start[1] + (end[1] - start[1]) * t)
        if release:
            self.button(False)

    def key_down(self, code):
        self._session("NotifyKeyboardKeycode", "(ub)", (code, True))

    def key_up(self, code):
        self._session("NotifyKeyboardKeycode", "(ub)", (code, False))

    def keys(self, *codes):
        """Presses keys together (modifiers first), then releases them."""
        for k in codes:
            self.key_down(k)
        for k in reversed(codes):
            self.key_up(k)

    def type(self, text):
        """Types `text` (letters, digits, spaces, a little punctuation)."""
        for c in text:
            code = self.TYPING[c.lower()]
            if c.isupper():
                self.keys(self.SHIFT, code)
            else:
                self.keys(code)

    def clipboard(self):
        """The clipboard's text. None while nothing owns the clipboard (for example
        before the app has handled the copy shortcut)."""
        try:
            r, fds = self.bus.call_with_unix_fd_list_sync(
                self.dest, self.session, self.iface, "SelectionRead",
                self.GLib.Variant("(s)", ("text/plain;charset=utf-8",)),
                None, 0, 5000, None, None,
            )
        except self.GLib.GError as e:
            if "No selection owner" in e.message:
                return None
            raise
        fd = fds.get(r.unpack()[0])
        os.set_blocking(fd, True)
        data = b""
        while chunk := os.read(fd, 65536):
            data += chunk
        os.close(fd)
        return data.decode()


class Window:
    def __init__(self, app, n):
        self.app, self.n = app, n
        self.path = f"{APP_PATH}/window/{n}"

    def action(self, name, param=None):
        gdbus_activate(self.path, name, param)

    def state(self):
        out = os.path.join(self.app.ctx.tmp, f"state-{self.n}.json")
        self.action("debug-state", gvariant_str(out))
        with open(out) as f:
            return json.load(f)

    def wait(self, what, pred, timeout=TIMEOUT):
        """Waits until `pred(state)` holds. Returns that state."""
        deadline = time.monotonic() + timeout
        while True:
            s = self.state()
            if pred(s):
                return s
            if time.monotonic() > deadline:
                raise CheckFailed(f"timed out waiting for {what}; state: {s}")
            time.sleep(0.05)

    def settled(self, what="the page to render", pred=lambda s: True):
        return self.wait(
            what, lambda s: s["view"] == "document" and s["complete"] and pred(s)
        )

    def maximize(self):
        """Maximizes the window, so window and screen coordinates agree."""
        self.action("debug-maximize", gvariant_str(""))
        self.wait("maximized", lambda s: s["maximized"])
        return self.stable()

    def stable(self, polls=4):
        """Waits until the window is rendered and its size and page
        positions stay the same for a few polls. GTK reports a window as
        maximized before it has the new size, and the last frame at the old
        size may still count as complete."""
        last, same = None, 0
        deadline = time.monotonic() + TIMEOUT
        while True:
            s = self.state()
            key = (s["width"], s["height"], json.dumps(s["pages"]))
            same = same + 1 if key == last and s["complete"] else 0
            if same >= polls:
                return s
            if time.monotonic() > deadline:
                raise CheckFailed(f"the window didn't settle; state: {s}")
            last = key
            time.sleep(0.1)

    def screen(self, page, x, y, state=None):
        """Window coordinates of PDF point (x, y) on `page`."""
        s = state or self.state()
        for p in s["pages"]:
            if p[0] == page:
                _, a, b, c, d, e, f = p
                return a * x + c * y + e, b * x + d * y + f
        raise CheckFailed(f"page {page} isn't near the current page: {s['pages']}")

    def char(self, page, line, col, before=True, state=None):
        """Window coordinates inside a character of TEXT (see text_point)."""
        return self.screen(page, *text_point(page, line, col, before), state=state)

    def on_canvas(self, x, y):
        """Whether window (x, y) is on the canvas, where the pointer's
        position can be read back."""
        s = self.state()
        cx, cy = s["canvas"]
        return cx <= x < s["width"] and cy <= y < s["height"]

    def pointer_at(self, x, y, timeout=TIMEOUT):
        """Waits until the canvas has seen the pointer at window (x, y)."""

        def there(s):
            px, py = s["pointer"][0] + s["canvas"][0], s["pointer"][1] + s["canvas"][1]
            return abs(px - x) < 1 and abs(py - y) < 1
        return self.wait(f"the pointer at {x:.0f}, {y:.0f}", there, timeout)

    def screenshot(self, name):
        path = os.path.join(self.app.ctx.out, f"{self.app.ctx.test}-{name}.ppm")
        if os.path.exists(path):
            os.remove(path)
        self.action("debug-screenshot", gvariant_str(path))
        # The app saves it after the next frame is painted, under another
        # name, then renames it: once it's there, it's complete.
        deadline = time.monotonic() + 5
        while not os.path.exists(path):
            if time.monotonic() > deadline:
                raise CheckFailed(f"no screenshot written to {path}")
            time.sleep(0.05)
        return path

    def colours(self, name):
        return page_colours(self.screenshot(name))

    def showing(self, page, name, share=0.25):
        """Checks that `page`'s color covers at least `share` of the window."""
        c = self.colours(name)
        check(
            c.get(page, 0) >= share,
            f"{name}: expected page {page} on screen, colours {fmt(c)}",
        )
        return c


def fmt(colours):
    return {p: round(s, 2) for p, s in sorted(colours.items())}


# Tests


def test_start_page(ctx):
    app = ctx.app()
    s = app.window().state()
    check(s["view"] == "empty", f"start page expected, got {s['view']}")
    app.quit()


def test_about(ctx):
    app = ctx.app()
    w = app.window()
    gdbus_activate(APP_PATH, "about")
    w.wait("the About dialog", lambda s: s["dialog"] == "About")
    time.sleep(0.5)  # the dialog's animation
    w.screenshot("about")
    # Closing the window closes the dialog first, so quitting takes two.
    gdbus_activate(APP_PATH, "quit")
    w.wait("the About dialog to go", lambda s: s["dialog"] == "")
    app.quit()


def test_properties(ctx):
    w = ctx.app([ctx.pages]).window()
    w.settled()
    inp = ctx.input().attach(w)
    check(w.state()["properties"] == [], "rows without the dialog")
    w.action("properties")
    s = w.wait("the Properties dialog", lambda s: s["dialog"] == "Properties")
    rows = dict(map(tuple, s["properties"]))
    check(rows["File Name"] == "pages.pdf" and rows["Folder"] == os.path.dirname(ctx.pages), rows)
    check(rows["Pages"] == str(PAGES) and rows["PDF Version"] == "1.7", rows)
    # One page is rotated.
    check(rows["Page Size"] == "8.5 × 11 in (Letter), other pages differ", rows)
    check(rows["File Size"].endswith("kB") and rows["Protection"] == "None", rows)
    # The file says nothing about itself.
    check("Title" not in rows and "Created" not in rows and "Not Allowed" not in rows, rows)
    time.sleep(0.5)  # the dialog's animation
    w.screenshot("properties")
    inp.keys(Input.ESCAPE)
    w.wait("the dialog to go", lambda s: s["dialog"] == "" and s["properties"] == [])


def test_translated(ctx):
    # The catalogs go where a build in the source tree looks for them.
    binary = os.path.dirname(ctx.binary)
    po = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "po")
    subprocess.run(
        [os.path.join(po, "install.sh"), os.path.join(binary, "..", "share", "locale")],
        check=True, stdout=subprocess.DEVNULL,
    )
    # In Dutch, whatever language the checks themselves run in. The C locale has no
    # translations, so the app runs in one that every system has, with Dutch asked for.
    known = subprocess.run(["locale", "-a"], capture_output=True, text=True).stdout.split()
    check("en_US.utf8" in known, "the en_US.UTF-8 locale isn't installed")
    ctx.env.update(LANGUAGE="nl", LC_ALL="en_US.UTF-8")
    w = ctx.app([ctx.pages]).window()
    s = w.settled()
    check(s["subtitle"] == f"{PAGES} pagina's", s["subtitle"])
    w.action("properties")
    s = w.wait("the Properties dialog", lambda s: s["dialog"] == "Eigenschappen")
    rows = dict(map(tuple, s["properties"]))
    check(rows.get("Pagina's") == str(PAGES) and rows.get("Beveiliging") == "Geen", rows)


def test_open_and_render(ctx):
    w = ctx.app([ctx.pages]).window()
    s = w.settled()
    check(s["title"] == "pages.pdf", s["title"])
    check(s["page_count"] == PAGES, s)
    check(s["page"] == 0 and s["page_entry"] == "i" and s["page_total"] == f"(1 of {PAGES})", s)
    check(s["layout"] == "continuous" and s["zoom_mode"] == "fit-width", s)
    check(s["sidebar"] and s["sidebar_page"] == "thumbnails", s)
    check(abs(s["scale"] - ctx.scale) < 0.01, f"scale {s['scale']}, expected {ctx.scale}")
    shot = w.screenshot("open")
    width, height, _ = read_ppm(shot)
    expect = (round(s["width"] * ctx.scale), round(s["height"] * ctx.scale))
    check(
        abs(width - expect[0]) <= 2 and abs(height - expect[1]) <= 2,
        f"screenshot {width}×{height}, expected {expect} (device pixels)",
    )
    w.showing(0, "open", share=0.4)


def test_navigation(ctx):
    w = ctx.app([ctx.pages]).window()
    w.settled()
    w.action("next-page")
    w.settled("page 2", lambda s: s["page"] == 1 and s["page_entry"] == "ii")
    w.action("last-page")
    s = w.settled("the last page", lambda s: s["page"] == PAGES - 1)
    check(s["page_entry"] == LABELS[-1], s)
    w.showing(PAGES - 1, "last")
    w.action("previous-page")
    w.settled("the second to last page", lambda s: s["page"] == PAGES - 2)
    w.action("first-page")
    w.settled("the first page", lambda s: s["page"] == 0)
    w.showing(0, "first")
    # The rotated page is landscape on screen and still rendered.
    w.action("debug-go-to", gvariant_str(LABELS[ROTATED]))
    s = w.settled("the rotated page", lambda s: s["page"] == ROTATED)
    w.showing(ROTATED, "rotated")


def test_page_entry(ctx):
    w = ctx.app([ctx.pages]).window()
    w.settled()
    # Labels win over page numbers: "4" is the page labeled 4.
    w.action("debug-go-to", gvariant_str("4"))
    s = w.settled("label 4", lambda s: s["page"] == LABELS.index("4"))
    check(s["page_entry"] == "4", s)
    w.showing(LABELS.index("4"), "label-4")
    # Case doesn't matter for roman labels.
    w.action("debug-go-to", gvariant_str("II"))
    w.settled("label ii", lambda s: s["page"] == 1)
    # Nonsense is refused, and the entry goes back to the current page.
    for text in ("999", "abc"):
        w.action("debug-go-to", gvariant_str(text))
        s = w.state()
        check(s["page"] == 1 and s["page_entry"] == "ii", f"after {text!r}: {s}")


def test_layouts(ctx):
    w = ctx.app([ctx.pages]).window()
    w.settled()
    target = 6
    w.action("debug-go-to", gvariant_str(LABELS[target]))
    w.settled("page 7", lambda s: s["page"] == target)
    for layout in ("two-page-continuous", "two-page", "single", "continuous"):
        w.action("layout", gvariant_str(layout))
        s = w.settled(
            layout, lambda s: s["layout"] == layout and abs(s["page"] - target) <= 1
        )
        c = w.showing(target, layout, share=0.1)
        if layout.startswith("two-page"):
            # Pages 7 and 8 side by side.
            check(c.get(target + 1, 0) > 0.1, f"{layout}: one page only: {fmt(c)}")
    w.action("layout", gvariant_str("two-page"))
    w.action("cover-alone")
    s = w.settled("cover alone", lambda s: s["cover_alone"])
    check(s["layout"] == "two-page", s)


def test_zoom(ctx):
    w = ctx.app([ctx.pages]).window()
    w.settled()
    w.action("zoom-actual")
    s = w.settled("actual size", lambda s: s["zoom_mode"] == "fixed")
    check(abs(s["zoom"] - 1.0) < 1e-9 and s["zoom_label"] == "100%", s)
    w.action("zoom-in")
    s = w.settled("zoom in", lambda s: s["zoom"] > 1.0)
    check(abs(s["zoom"] - 1.25) < 1e-9 and s["zoom_label"] == "125%", s)
    w.action("zoom-out")
    w.settled("zoom out", lambda s: abs(s["zoom"] - 1.0) < 1e-9)
    w.action("layout", gvariant_str("single"))
    w.action("fit-page")
    s = w.settled("fit page", lambda s: s["zoom_mode"] == "fit-page")
    # The whole page is visible: its colored area, 540 by 690 of 612 by 792
    # points, at the current zoom.
    c = w.showing(0, "fit-page", share=0.0)
    area = 540 * 690 * s["zoom"] ** 2 / (s["width"] * s["height"])
    check(abs(c.get(0, 0) - area) < 0.05, f"page area {fmt(c)}, expected {area:.2f}")
    # Limits: 5% to 3200%, and it still renders there.
    for _ in range(30):
        w.action("zoom-in")
    s = w.settled("maximum zoom", lambda s: s["zoom"] >= 32 - 1e-9)
    check(s["zoom_label"] == "3200%", s)
    w.showing(0, "max-zoom", share=0.5)
    for _ in range(40):
        w.action("zoom-out")
    s = w.settled("minimum zoom", lambda s: s["zoom"] <= 0.05 + 1e-9)
    check(s["zoom_label"] == "5%", s)


def test_sidebar(ctx):
    w = ctx.app([ctx.pages]).window()
    w.settled()
    w.action("show-sidebar")
    w.settled("hidden sidebar", lambda s: not s["sidebar"])
    w.action("sidebar-page", gvariant_str("outline"))
    w.settled("contents", lambda s: s["sidebar"] and s["sidebar_page"] == "outline")


def test_remembers_state(ctx):
    app = ctx.app([ctx.pages])
    w = app.window()
    w.settled()
    w.action("debug-go-to", gvariant_str("6"))
    w.action("layout", gvariant_str("two-page"))
    w.action("zoom-actual")
    w.action("sidebar-page", gvariant_str("outline"))
    before = w.settled("changes", lambda s: s["layout"] == "two-page" and s["zoom_mode"] == "fixed")
    app.quit()

    w = ctx.app([ctx.pages]).window()
    s = w.settled("restored state", lambda s: s["layout"] == "two-page")
    for key in ("page", "layout", "zoom_mode", "zoom", "sidebar", "sidebar_page"):
        check(s[key] == before[key], f"{key}: {s[key]!r}, before closing {before[key]!r}")


def test_broken_file(ctx):
    w = ctx.app([ctx.broken]).window()
    s = w.wait("an error", lambda s: s["toast"] != "")
    check(s["view"] == "empty", s)
    check(s["toast"].startswith("Couldn't open “broken.pdf”"), s["toast"])


def test_two_windows(ctx):
    app = ctx.app([ctx.pages, ctx.pages])
    one, two = app.window(1), app.window(2)
    # Only the window on top is sure to draw: on a small screen it covers
    # the other one, and the compositor stops sending frames to hidden
    # windows. So wait for the documents to load, not to render.
    for w in (one, two):
        w.wait("the document", lambda s: s["view"] == "document")
    one.action("last-page")
    one.wait("last page", lambda s: s["page"] == PAGES - 1)
    check(two.state()["page"] == 0, "the second window moved too")


def selection_window(ctx):
    """A maximized window on the text PDF, and an input device."""
    w = ctx.app([ctx.text]).window()
    w.settled()
    w.maximize()
    return w, ctx.input().attach(w)


def selected(w, text):
    return w.wait(f"selection {text!r}", lambda s: s["selection"] == text)


def test_link_click(ctx):
    w, inp = selection_window(ctx)
    check(w.state()["history"] == [False, False], "a place to go back to at the start")
    inp.click(*w.char(0, LINK_LINE, 3))
    s = w.wait("the link to page 2", lambda s: s["page"] == 1)
    check(s["history"] == [True, False], s["history"])
    # Back returns to where the link was clicked, and Forward to its target.
    inp.keys(Input.ALT, Input.LEFT)
    w.wait("back on page 1", lambda s: s["page"] == 0 and s["history"] == [False, True])
    inp.keys(Input.ALT, Input.RIGHT)
    w.wait("forward on page 2", lambda s: s["page"] == 1 and s["history"] == [True, False])


def test_back_and_forward(ctx):
    w = ctx.app([ctx.pages]).window()
    w.settled()
    at = lambda page, history: lambda s: s["page"] == page and s["history"] == history
    # Turning pages isn't a jump.
    w.action("next-page")
    w.wait("page 2", at(1, [False, False]))
    # The last page, a page by its number, and an outline entry are.
    w.action("last-page")
    w.wait("the last page", at(PAGES - 1, [True, False]))
    w.action("debug-go-to", gvariant_str("ii"))
    w.wait("page ii", at(1, [True, False]))
    w.action("go-back")
    w.wait("back on the last page", at(PAGES - 1, [True, True]))
    w.action("go-back")
    w.wait("back on page 2", at(1, [False, True]))
    w.action("go-forward")
    w.action("go-forward")
    w.wait("forward again", at(1, [True, False]))
    # A jump after going back drops what was ahead.
    w.action("go-back")
    w.wait("back on the last page", at(PAGES - 1, [True, True]))
    w.action("first-page")
    w.wait("the first page", at(0, [True, False]))
    w.action("go-back")
    w.action("go-back")
    w.wait("back at the start", at(1, [False, True]))


def test_select_by_dragging(ctx):
    w, inp = selection_window(ctx)
    # Over text the cursor is an I-beam.
    x, y = w.char(0, 0, 6)
    inp.move(x, y)
    s = w.pointer_at(x, y)
    check(s["cursor"] == "text", f"cursor over text: {s['cursor']!r}")
    # From the start of "quick" (character 4) to the end of "brown" (14).
    inp.drag(w.char(0, 0, 4), w.char(0, 0, 14, before=False))
    selected(w, "quick brown")
    w.screenshot("selected")
    # Backwards and over two lines: from the end of "lazy" back to the
    # start of "brown".
    inp.drag(w.char(0, 1, 18, before=False), w.char(0, 0, 10))
    selected(w, "brown fox\njumps over the lazy")
    # Escape clears it, and so does a click.
    inp.keys(Input.ESCAPE)
    selected(w, "")
    inp.drag(w.char(0, 0, 0), w.char(0, 0, 2, before=False))
    selected(w, "The")
    inp.click(*w.screen(0, 300, 300))
    selected(w, "")


def pixel(w, name, x, y):
    """The color of the window at (x, y), from a screenshot."""
    width, height, pixels = read_ppm(w.screenshot(name))
    scale = w.app.ctx.scale
    at = (min(round(y * scale), height - 1) * width + min(round(x * scale), width - 1)) * 3
    return tuple(pixels[at:at + 3])


def yellow(c):
    return c[0] > 200 and c[1] > 180 and c[2] < 120


def red(c):
    return c[0] > 150 and c[1] < 110 and c[2] < 110


def underlined(w, name, page, line, col):
    """Whether there is red just below or at the baseline of character
    `col`, where an underline is drawn."""
    width, height, pixels = read_ppm(w.screenshot(name))
    scale = w.app.ctx.scale
    x = LEFT + (col + 0.5) * CHAR
    for dy in range(-6, 3):
        sx, sy = w.screen(page, x, TOP - line * LEADING + dy)
        at = (min(round(sy * scale), height - 1) * width + min(round(sx * scale), width - 1)) * 3
        if red(tuple(pixels[at:at + 3])):
            return True
    return False


def test_mark_text(ctx):
    path = os.path.join(ctx.home, "marked.pdf")
    shutil.copy(ctx.text, path)
    app = ctx.app([path])
    w = app.window()
    w.settled()
    w.maximize()
    inp = ctx.input().attach(w)
    # Nothing selected: nothing happens, and a message says why.
    w.action("highlight")
    s = w.wait("the message", lambda s: s["toast"] != "")
    check(s["annotations"] == 0 and not s["modified"], s)
    # "quick brown" highlighted: the gap between the words turns yellow,
    # the selection is given up, and it can be undone by name.
    inp.drag(w.char(0, 0, 4), w.char(0, 0, 14, before=False))
    selected(w, "quick brown")
    w.action("highlight")
    s = w.wait("the highlight", lambda s: s["annotations"] == 1)
    check(s["selection"] == "" and s["modified"] and s["undo"] == "Add Highlight", s)
    gap = w.char(0, 0, 9)
    check(yellow(pixel(w, "highlight", *gap)), f"no highlight at the gap: {pixel(w, 'gap', *gap)}")
    beside = w.char(0, 0, 15)
    check(not yellow(pixel(w, "beside", *beside)), "the highlight is too long")
    # A line under "fox" as well.
    inp.drag(w.char(0, 0, 16), w.char(0, 0, 18, before=False))
    selected(w, "fox")
    w.action("underline")
    w.wait("the underline", lambda s: s["annotations"] == 2 and s["undo"] == "Add Underline")
    # Printed before it is saved: the highlight is on the paper.
    out = os.path.join(ctx.home, "marked-print.pdf")
    w.action("debug-print-to-file", gvariant_str(out))
    w.wait("the print", lambda s: s["printed"] == 1)
    prefix = os.path.join(ctx.home, "marked-print")
    subprocess.run(["pdftoppm", "-r", "40", "-f", "1", "-l", "1", "-singlefile", out, prefix],
                   check=True)
    _, _, pixels = read_ppm(prefix + ".ppm")
    marked = sum(yellow(tuple(pixels[k:k + 3])) for k in range(0, len(pixels), 3))
    check(marked > 20, f"the print has {marked} yellow pixels")
    # A click picks the highlight. Delete deletes it, not the page.
    inp.click(*gap)
    w.wait("the highlight picked", lambda s: s["annotation_picked"])
    w.screenshot("picked")
    inp.keys(Input.DELETE)
    s = w.wait("the highlight gone", lambda s: s["annotations"] == 1)
    check(s["undo"] == "Remove Highlight" and not s["annotation_picked"], s)
    check(s["page_count"] == len(TEXT), "Delete deleted a page")
    check(not yellow(pixel(w, "deleted", *gap)), "the highlight is still drawn")
    w.action("undo")
    w.wait("the highlight back", lambda s: s["annotations"] == 2)
    check(yellow(pixel(w, "undone", *gap)), "the highlight didn't come back")
    # Escape lets go of a picked one.
    inp.click(*gap)
    w.wait("the highlight picked", lambda s: s["annotation_picked"])
    inp.keys(Input.ESCAPE)
    w.wait("nothing picked", lambda s: not s["annotation_picked"])
    # Saved and opened again: both are in the file, and shown.
    w.action("save")
    w.wait("the save", lambda s: not s["modified"] and not s["saving"])
    app.quit()
    w = ctx.app([path]).window()
    s = w.settled()
    w.maximize()
    check(s["annotations"] == 2 and not s["modified"], s)
    gap = w.char(0, 0, 9)
    check(yellow(pixel(w, "reopened", *gap)), "the saved highlight isn't shown")
    # Now that the file has them, they are part of the page as rendered.
    # Deleting one renders the page without it. The underline stays.
    inp = ctx.input().attach(w)
    inp.click(*gap)
    w.wait("the saved highlight picked", lambda s: s["annotation_picked"])
    inp.keys(Input.DELETE)
    s = w.wait("the saved highlight gone", lambda s: s["annotations"] == 1)
    check(s["modified"] and s["undo"] == "Remove Highlight", s)
    check(s["page_count"] == len(TEXT), "Delete deleted a page")
    w.settled()
    check(not yellow(pixel(w, "file-deleted", *gap)), "the deleted highlight is still shown")
    check(underlined(w, "file-underline", 0, 0, 17), "the underline went with it")
    w.action("undo")
    w.wait("the saved highlight back", lambda s: s["annotations"] == 2)
    w.settled()
    check(yellow(pixel(w, "file-undone", *gap)), "the highlight didn't come back")
    w.action("redo")
    w.wait("gone again", lambda s: s["annotations"] == 1)
    w.action("save")
    w.wait("the save", lambda s: not s["modified"] and not s["saving"])
    w.app.quit()
    w = ctx.app([path]).window()
    s = w.settled()
    w.maximize()
    check(s["annotations"] == 1, s)
    check(not yellow(pixel(w, "file-saved", *w.char(0, 0, 9))), "the highlight is in the saved file")


def test_markup_bar(ctx):
    w, inp = selection_window(ctx)
    s = w.state()
    check(not s["markup_bar"] and s["markup_tool"] == "none" and s["markup_color"] == "yellow", s)
    w.action("show-markup")
    w.wait("the markup bar", lambda s: s["markup_bar"])
    time.sleep(0.4)  # the bar slides in and moves the page down
    # With the highlighter pressed in, text is marked as it is selected, by
    # dragging and by a double click.
    w.action("markup-tool", gvariant_str("highlight"))
    w.wait("the highlighter", lambda s: s["markup_tool"] == "highlight")
    inp.drag(w.char(0, 0, 4), w.char(0, 0, 8, before=False))
    s = w.wait("quick marked", lambda s: s["annotations"] == 1)
    check(s["selection"] == "" and s["undo"] == "Add Highlight", s)
    inp.click(*w.char(0, 1, 1), times=2)
    w.wait("a word marked", lambda s: s["annotations"] == 2)
    quick = w.char(0, 0, 6)
    check(yellow(pixel(w, "yellow", *quick)), "not highlighted in yellow")
    # Another color for what is marked next.
    w.action("markup-color", gvariant_str("green"))
    w.wait("green", lambda s: s["markup_color"] == "green")
    inp.drag(w.char(0, 0, 10), w.char(0, 0, 14, before=False))
    w.wait("brown marked", lambda s: s["annotations"] == 3)
    brown = w.char(0, 0, 12)
    c = pixel(w, "green", *brown)
    check(c[1] > c[0] + 30 and c[1] > c[2] + 30, f"not highlighted in green: {c}")
    # The underline tool has its own color, red. The bar shows it.
    w.action("markup-tool", gvariant_str("underline"))
    s = w.wait("the underliner", lambda s: s["markup_tool"] == "underline")
    check(s["markup_color"] == "red", s)
    # A picked annotation shows its color, and takes the one chosen then.
    inp.click(*quick)
    s = w.wait("quick picked", lambda s: s["annotation_picked"])
    check(s["markup_color"] == "yellow", s)
    w.action("markup-color", gvariant_str("blue"))
    s = w.wait("recoloured", lambda s: s["undo"] == "Change Highlight")
    check(s["annotations"] == 3 and s["annotation_picked"] and s["markup_color"] == "blue", s)
    inp.keys(Input.ESCAPE)
    s = w.wait("let go", lambda s: not s["annotation_picked"])
    check(s["markup_color"] == "red", s)
    c = pixel(w, "blue", *quick)
    check(c[2] > c[0] + 30, f"not recoloured to blue: {c}")
    w.screenshot("bar")
    # Activating the tool that is in lets go of it. So does closing the bar.
    w.action("markup-tool", gvariant_str("underline"))
    w.wait("no tool", lambda s: s["markup_tool"] == "none")
    w.action("markup-tool", gvariant_str("strike-out"))
    w.wait("strike-out", lambda s: s["markup_tool"] == "strike-out")
    w.action("show-markup")
    s = w.wait("the bar closed", lambda s: not s["markup_bar"] and s["markup_tool"] == "none")
    time.sleep(0.4)
    inp.drag(w.char(0, 1, 6), w.char(0, 1, 9, before=False))
    time.sleep(0.5)
    check(w.state()["annotations"] == 3, "text was marked without a tool")


def test_draw_tools(ctx):
    path = os.path.join(ctx.home, "drawn.pdf")
    shutil.copy(ctx.text, path)
    app = ctx.app([path])
    w = app.window()
    w.settled()
    w.maximize()
    inp = ctx.input().attach(w)
    w.action("show-markup")
    w.wait("the markup bar", lambda s: s["markup_bar"])
    time.sleep(0.4)  # the bar slides in and moves the page down
    at = lambda x, y: w.screen(0, x, y)
    # The pen: a drag below the text draws instead of selecting, in red.
    w.action("markup-tool", gvariant_str("draw"))
    s = w.wait("the pen", lambda s: s["markup_tool"] == "draw")
    check(s["markup_color"] == "red", s)
    inp.move(*at(100, 520))
    check(w.pointer_at(*at(100, 520))["cursor"] == "crosshair", "no crosshair with the pen")
    inp.drag(at(100, 520), at(260, 520))
    s = w.wait("a stroke", lambda s: s["annotations"] == 1)
    check(s["undo"] == "Add Ink" and s["selection"] == "" and s["modified"], s)
    check(red(pixel(w, "stroke", *at(180, 520))), "no stroke where the pen went")
    # Over text too: nothing is selected.
    inp.drag(w.char(0, 2, 2), w.char(0, 2, 12))
    s = w.wait("a stroke over text", lambda s: s["annotations"] == 2)
    check(s["selection"] == "", s)
    # A rectangle in blue, an ellipse and a line: from corner to corner.
    w.action("markup-tool", gvariant_str("rectangle"))
    w.action("markup-color", gvariant_str("blue"))
    w.wait("blue rectangles", lambda s: s["markup_tool"] == "rectangle" and s["markup_color"] == "blue")
    inp.drag(at(100, 480), at(200, 400))
    s = w.wait("a rectangle", lambda s: s["annotations"] == 3 and s["undo"] == "Add Square")
    blue = lambda c: c[2] > c[0] + 30
    check(blue(pixel(w, "rectangle", *at(150, 480))), "no line at the rectangle's top")
    check(not blue(pixel(w, "inside", *at(150, 440))), "the rectangle is filled")
    w.action("markup-tool", gvariant_str("ellipse"))
    w.wait("ellipses", lambda s: s["markup_tool"] == "ellipse" and s["markup_color"] == "red")
    inp.drag(at(250, 480), at(350, 400))
    w.wait("an ellipse", lambda s: s["annotations"] == 4 and s["undo"] == "Add Circle")
    check(red(pixel(w, "ellipse", *at(300, 480))), "no line at the ellipse's top")
    w.action("markup-tool", gvariant_str("line"))
    inp.drag(at(400, 480), at(500, 400))
    w.wait("a line", lambda s: s["annotations"] == 5 and s["undo"] == "Add Line")
    check(red(pixel(w, "line", *at(450, 440))), "no line between its ends")
    # A click with a shape tool draws nothing.
    inp.click(*at(300, 300))
    time.sleep(0.4)
    check(w.state()["annotations"] == 5, "a click drew a shape")
    w.screenshot("drawn")
    # Without a tool, a click picks the line (near it, not in the area
    # around it), and Delete deletes it.
    w.action("markup-tool", gvariant_str("line"))
    w.wait("no tool", lambda s: s["markup_tool"] == "none")
    inp.click(*at(410, 410))
    time.sleep(0.4)
    check(not w.state()["annotation_picked"], "picked far from the line")
    inp.click(*at(450, 440))
    w.wait("the line picked", lambda s: s["annotation_picked"])
    inp.keys(Input.DELETE)
    w.wait("the line gone", lambda s: s["annotations"] == 4)
    w.action("undo")
    w.wait("the line back", lambda s: s["annotations"] == 5)
    # A drag from inside the rectangle moves it (60 to the right), without
    # picking it first. It is picked then, and the pointer says "move".
    inp.keys(Input.ESCAPE)
    w.wait("nothing picked", lambda s: not s["annotation_picked"])
    inp.drag(at(150, 440), at(210, 440))
    s = w.wait("the rectangle moved", lambda s: s["undo"] == "Change Square")
    check(s["annotation_picked"] and s["annotations"] == 5 and s["selection"] == "", s)
    check(blue(pixel(w, "moved", *at(210, 480))), "the rectangle isn't where it was moved to")
    check(not blue(pixel(w, "moved-from", *at(110, 480))), "the rectangle is still where it was")
    # (The pointer's shape follows its movement: moved once more, in case
    # the first move got there before the rectangle did.)
    inp.move(*at(210, 440))
    w.pointer_at(*at(210, 440))
    x, y = at(212, 440)
    inp.move(x, y)
    w.wait("the move cursor on the picked rectangle", lambda s: s["cursor"] == "move")
    # Its lower right corner dragged down: its top stays, its bottom goes.
    inp.drag(at(260, 400), at(260, 360))
    w.wait("the rectangle resized", lambda s: s["annotations"] == 5 and not s["saving"])
    time.sleep(0.3)
    check(blue(pixel(w, "resized", *at(210, 360))), "the bottom didn't follow the corner")
    check(blue(pixel(w, "resized-top", *at(210, 480))), "the top moved too")
    check(not blue(pixel(w, "resized-old", *at(210, 400))), "the old bottom is still drawn")
    # Escape during a drag puts it back.
    inp.drag(at(210, 420), at(300, 420), release=False)
    inp.keys(Input.ESCAPE)
    inp.button(False)
    time.sleep(0.3)
    check(blue(pixel(w, "given-up", *at(210, 480))), "Escape didn't put the rectangle back")
    # Saved and opened again: all there, drawn with the page now.
    w.action("save")
    w.wait("the save", lambda s: not s["modified"] and not s["saving"])
    app.quit()
    app = ctx.app([path])
    w = app.window()
    s = w.settled()
    w.maximize()
    check(s["annotations"] == 5 and not s["modified"], s)
    check(red(pixel(w, "reopened", *w.screen(0, 180, 520))), "the saved stroke isn't shown")
    at = lambda x, y: w.screen(0, x, y)
    check(blue(pixel(w, "reopened-rectangle", *at(210, 480))), "the saved rectangle isn't shown")
    # Now that the file has it, the rectangle moves just the same, and
    # nothing is left where it was.
    inp = ctx.input().attach(w)
    inp.drag(at(210, 420), at(310, 420))
    s = w.wait("the saved rectangle moved", lambda s: s["undo"] == "Change Square")
    w.settled()
    check(blue(pixel(w, "file-moved", *at(310, 480))), "the saved rectangle didn't move")
    check(not blue(pixel(w, "file-moved-from", *at(180, 480))), "the saved rectangle is still drawn where it was")
    w.action("undo")
    w.wait("moved back", lambda s: not s["modified"])
    w.settled()
    check(blue(pixel(w, "file-back", *at(210, 480))), "undo didn't put the saved rectangle back")
    check(not blue(pixel(w, "file-back-from", *at(350, 480))), "undo left the moved rectangle")


def dark_pixels(w, name, top_left, bottom_right):
    """How many dark pixels the window has between two points (text)."""
    width, height, pixels = read_ppm(w.screenshot(name))
    scale = w.app.ctx.scale
    (x0, y0), (x1, y1) = top_left, bottom_right
    count = 0
    for y in range(round(y0 * scale), min(round(y1 * scale), height)):
        for x in range(round(x0 * scale), min(round(x1 * scale), width)):
            at = (y * width + x) * 3
            count += max(pixels[at:at + 3]) < 110
    return count


def test_text_tools(ctx):
    path = os.path.join(ctx.home, "texts.pdf")
    shutil.copy(ctx.text, path)
    app = ctx.app([path])
    w = app.window()
    w.settled()
    w.maximize()
    inp = ctx.input().attach(w)
    w.action("show-markup")
    w.wait("the markup bar", lambda s: s["markup_bar"])
    time.sleep(0.4)  # the bar slides in and moves the page down
    at = lambda x, y: w.screen(0, x, y)
    closed = lambda s: not s["text_editor"]
    # A note where the page is clicked: its text is asked for at once, and
    # kept when the editor is closed.
    w.action("markup-tool", gvariant_str("note"))
    w.wait("the note tool", lambda s: s["markup_tool"] == "note" and s["markup_color"] == "yellow")
    inp.click(*at(400, 500))
    s = w.wait("a note and its editor", lambda s: s["annotations"] == 1 and s["text_editor"])
    check(s["undo"] == "Add Note" and s["annotation_picked"], s)
    time.sleep(0.4)  # the popover takes the keyboard once it is up
    w.screenshot("note-editor")
    inp.type("check this")
    inp.keys(Input.ESCAPE)
    s = w.wait("the note's text", lambda s: closed(s) and s["annotation_text"] == "check this")
    check(yellow(pixel(w, "note", *at(414, 489))), "no note where the page was clicked")
    # Text: asked for where the page is clicked, a text box once there is
    # some. Nothing without.
    w.action("markup-tool", gvariant_str("text"))
    w.wait("the text tool", lambda s: s["markup_tool"] == "text" and s["markup_color"] == "black")
    inp.click(*at(100, 450))
    s = w.wait("the editor", lambda s: s["text_editor"])
    check(s["annotations"] == 1, s)
    time.sleep(0.4)
    inp.keys(Input.ESCAPE)
    s = w.wait("no text, no box", closed)
    check(s["annotations"] == 1 and s["undo"] == "Change Note", s)
    area = at(100, 452), at(330, 420)
    before = dark_pixels(w, "before", *area)
    inp.click(*at(100, 450))
    w.wait("the editor", lambda s: s["text_editor"])
    time.sleep(0.4)
    inp.type("some words")
    w.screenshot("typing")
    inp.keys(Input.ESCAPE)
    s = w.wait("a text box", lambda s: closed(s) and s["annotations"] == 2)
    check(s["undo"] == "Add FreeText" and s["annotation_text"] == "some words", s)
    w.settled()
    one_line = dark_pixels(w, "text-box", *area)
    check(one_line > before + 40, f"no text on the page: {before} then {one_line} dark pixels")
    # Without a tool: a double click on the text box asks for its text
    # again, and more text is more ink.
    w.action("markup-tool", gvariant_str("text"))
    w.wait("no tool", lambda s: s["markup_tool"] == "none")
    inp.click(*at(110, 442), times=2)
    w.wait("the editor again", lambda s: s["text_editor"])
    time.sleep(0.4)
    inp.type(" and more")
    inp.keys(Input.ESCAPE)
    s = w.wait("more text", lambda s: closed(s) and s["annotation_text"] != "some words")
    check(sorted(s["annotation_text"].split()) == sorted("some words and more".split()), s)
    w.settled()
    more = dark_pixels(w, "more-text", *area)
    check(more > one_line + 20, f"no more text on the page: {one_line} then {more}")
    # A click on a note shows its text, to read or change. Enter on a
    # picked annotation does the same.
    inp.click(*at(409, 491))
    s = w.wait("the note's editor", lambda s: s["text_editor"])
    check(s["annotation_text"] == "check this", s)
    time.sleep(0.4)
    inp.keys(Input.ESCAPE)
    s = w.wait("closed", closed)
    check(s["annotation_picked"] and s["annotation_text"] == "check this", s)
    inp.keys(Input.ENTER)
    w.wait("the note's editor again", lambda s: s["text_editor"])
    time.sleep(0.4)
    inp.keys(Input.ESCAPE)
    w.wait("closed", closed)
    # A drag moves the text box, text and all.
    inp.drag(at(120, 442), at(120, 362))
    w.wait("the text box moved", lambda s: s["undo"] == "Change FreeText")
    w.settled()
    check(dark_pixels(w, "moved-from", *area) <= before + 5, "text is left where the box was")
    moved = at(100, 372), at(330, 340)
    check(dark_pixels(w, "moved-to", *moved) > 60, "no text where the box was moved to")
    # Saved and opened again: both there, with their text, which can still
    # be changed. A text box emptied is deleted.
    w.action("save")
    w.wait("the save", lambda s: not s["modified"] and not s["saving"])
    app.quit()
    w = ctx.app([path]).window()
    s = w.settled()
    w.maximize()
    check(s["annotations"] == 2 and not s["modified"], s)
    at = lambda x, y: w.screen(0, x, y)
    inp = ctx.input().attach(w)
    inp.click(*at(120, 362), times=2)
    s = w.wait("the saved text box's editor", lambda s: s["text_editor"])
    check(sorted(s["annotation_text"].split()) == sorted("some words and more".split()), s)
    time.sleep(0.4)
    inp.keys(Input.CONTROL, Input.KEY_A)
    inp.keys(Input.BACKSPACE)
    inp.keys(Input.ESCAPE)
    s = w.wait("the text box gone", lambda s: closed(s) and s["annotations"] == 1)
    check(s["undo"] == "Remove FreeText", s)
    w.settled()
    check(dark_pixels(w, "emptied", at(100, 372), at(330, 340)) < 10, "the deleted text is still shown")


def test_markup_list(ctx):
    path = os.path.join(ctx.home, "listed.pdf")
    shutil.copy(ctx.text, path)
    app = ctx.app([path])
    w = app.window()
    w.settled()
    w.maximize()
    inp = ctx.input().attach(w)
    # The Markup page of the sidebar: empty to begin with.
    w.action("sidebar-page", gvariant_str("markup"))
    s = w.wait("the markup list", lambda s: s["sidebar_page"] == "markup")
    check(s["markup_list"] == [], s)
    # A highlight is listed with the text it marks, a rectangle as what it
    # is, each with its page.
    inp.drag(w.char(0, 0, 4), w.char(0, 0, 14, before=False))
    selected(w, "quick brown")
    w.action("highlight")
    s = w.wait("the highlight listed", lambda s: len(s["markup_list"]) == 1)
    check(s["markup_list"][0] == ["1", "Highlight", "quick brown"], s["markup_list"])
    w.action("show-markup")
    w.wait("the markup bar", lambda s: s["markup_bar"])
    time.sleep(0.4)
    w.action("markup-tool", gvariant_str("rectangle"))
    inp.drag(w.screen(0, 100, 480), w.screen(0, 200, 400))
    s = w.wait("the rectangle listed", lambda s: len(s["markup_list"]) == 2)
    check(s["markup_list"][1] == ["1", "Rectangle", ""], s["markup_list"])
    w.action("markup-tool", gvariant_str("rectangle"))
    # A row picks its annotation. A comment typed for it shows in the list.
    w.action("debug-markup-row", gvariant_str("1"))
    w.wait("the rectangle picked", lambda s: s["annotation_picked"] and s["markup_color"] == "red")
    w.action("debug-markup-row", gvariant_str("0"))
    w.wait("the highlight picked", lambda s: s["annotation_text"] == "quick brown")
    w.action("debug-markup-row", gvariant_str("1"))
    w.wait("the rectangle picked", lambda s: s["annotation_text"] == "")
    inp.keys(Input.ENTER)
    w.wait("its editor", lambda s: s["text_editor"])
    time.sleep(0.4)
    inp.type("a box")
    inp.keys(Input.ESCAPE)
    s = w.wait("the comment listed", lambda s: s["markup_list"][1][2] == "a box")
    # Deleted and undone, the list follows.
    inp.keys(Input.DELETE)
    w.wait("one row", lambda s: len(s["markup_list"]) == 1)
    w.action("undo")
    s = w.wait("two rows", lambda s: len(s["markup_list"]) == 2)
    # With the keyboard in the list, Backspace deletes the annotation of the
    # marked row, then that of the row taking its place, and never pages.
    x, y, width, height = s["markup_rows"][0]
    # From a place on the canvas, where the pointer's position is checked.
    inp.move(s["width"] * 0.75, s["height"] / 2)
    inp.click(x + width / 2, y + height / 2)
    w.wait("the highlight picked", lambda s: s["annotation_text"] == "quick brown")
    inp.keys(Input.BACKSPACE)
    s = w.wait("one row", lambda s: len(s["markup_list"]) == 1)
    check(s["undo"] == "Remove Highlight" and s["markup_list"][0][1] == "Rectangle", s)
    inp.keys(Input.BACKSPACE)
    w.wait("no row", lambda s: len(s["markup_list"]) == 0)
    inp.keys(Input.BACKSPACE)
    time.sleep(0.3)
    s = w.state()
    check(len(s["edit_pages"]) == len(TEXT) and s["undo"].startswith("Remove"), s)
    w.action("undo")
    w.action("undo")
    w.wait("two rows", lambda s: len(s["markup_list"]) == 2)
    # Saved and opened again: the sidebar is on Markup, with what the file
    # has. From page 3, a row leads back to its page.
    w.action("save")
    w.wait("the save", lambda s: not s["modified"] and not s["saving"])
    app.quit()
    w = ctx.app([path]).window()
    s = w.settled("the list from the file", lambda s: len(s["markup_list"]) == 2)
    check(s["sidebar_page"] == "markup", s)
    check(s["markup_list"] == [["1", "Highlight", "quick brown"], ["1", "Rectangle", "a box"]],
          s["markup_list"])
    w.action("last-page")
    w.wait("the last page", lambda s: s["page"] == len(TEXT) - 1)
    w.action("debug-markup-row", gvariant_str("0"))
    w.wait("back on page 1", lambda s: s["page"] == 0 and s["annotation_picked"])


def write_png(path, width, height, rgba):
    """A PNG of one color (red, green, blue, alpha)."""
    import struct
    import zlib

    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))

    rows = b"".join(b"\x00" + bytes(rgba) * width for _ in range(height))
    with open(path, "wb") as f:
        f.write(b"\x89PNG\r\n\x1a\n")
        f.write(chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)))
        f.write(chunk(b"IDAT", zlib.compress(rows)))
        f.write(chunk(b"IEND", b""))


def test_signatures(ctx):
    # Two saved signatures, as the app keeps them: one drawn (a line across
    # and one down, in a box of 100 by 40) and one from a photo (blue ink).
    kept = os.path.join(ctx.env["XDG_DATA_HOME"], "specto", "signatures")
    os.makedirs(kept)
    with open(os.path.join(kept, "1-drawn.strokes"), "w") as f:
        f.write("specto-signature 1\nsize 100 40\n0 20 50 20 100 20\n50 0 50 40\n")
    write_png(os.path.join(kept, "2-photo.png"), 40, 10, (0, 0, 255, 255))
    path = os.path.join(ctx.home, "signed.pdf")
    shutil.copy(ctx.text, path)
    app = ctx.app([path])
    w = app.window()
    s = w.settled()
    check(s["signatures"] == 2, s)
    w.maximize()
    inp = ctx.input().attach(w)
    w.action("show-markup")
    w.wait("the markup bar", lambda s: s["markup_bar"])
    time.sleep(0.4)
    at = lambda x, y: w.screen(0, x, y)
    blue = lambda c: c[2] > 150 and c[0] < 100 and c[1] < 100
    dark = lambda c: max(c) < 110
    # The drawn one, chosen and then placed by a click: a drawing, 150
    # points wide, with its middle where the click was.
    w.action("debug-place-signature", gvariant_str("0"))
    w.wait("placing", lambda s: s["placing_signature"])
    inp.move(*at(300, 520))
    check(w.pointer_at(*at(300, 520))["cursor"] == "crosshair", "no crosshair while placing")
    inp.click(*at(300, 520))
    s = w.wait("the signature placed", lambda s: s["annotations"] == 1)
    check(s["undo"] == "Add Ink" and s["annotation_picked"] and not s["placing_signature"], s)
    check(dark(pixel(w, "drawn", *at(260, 520))), "no stroke left of the middle")
    check(dark(pixel(w, "drawn-down", *at(300, 500))), "no stroke below the middle")
    check(not dark(pixel(w, "drawn-beside", *at(260, 500))), "ink where no stroke is")
    # Once: the next click places nothing.
    inp.click(*at(450, 300))
    time.sleep(0.4)
    check(w.state()["annotations"] == 1, "a second click placed another")
    # The one from a photo: a picture, 150 wide and a quarter as high.
    w.action("debug-place-signature", gvariant_str("1"))
    w.wait("placing", lambda s: s["placing_signature"])
    inp.click(*at(300, 400))
    s = w.wait("the picture placed", lambda s: s["annotations"] == 2 and s["undo"] == "Add Stamp")
    w.settled()
    check(blue(pixel(w, "photo", *at(300, 400))), "no picture where it was placed")
    check(blue(pixel(w, "photo-left", *at(235, 400))), "the picture isn't 150 wide")
    check(not blue(pixel(w, "photo-above", *at(300, 425))), "the picture is too high")
    # It moves, and keeps its proportions when resized by a corner: pulled
    # to twice the width, it is twice as high too.
    inp.drag(at(300, 400), at(300, 340))
    w.wait("the picture moved", lambda s: s["undo"] == "Change Stamp")
    w.settled()
    check(blue(pixel(w, "moved", *at(300, 340))), "the picture didn't move")
    check(not blue(pixel(w, "moved-from", *at(300, 405))), "the picture is still where it was")
    inp.drag(at(375, 321), at(525, 321))
    w.settled()
    time.sleep(0.3)
    check(blue(pixel(w, "resized", *at(500, 345))), "the picture didn't grow to the right")
    # (Its top stays, being across from the corner pulled.)
    # Sampled just below where its bottom was: further down is off screen
    # in a small window.
    check(blue(pixel(w, "resized-down", *at(300, 312))), "the picture didn't grow in height")
    check(not blue(pixel(w, "resized-top", *at(300, 375))), "the picture's top moved")
    # Saved and opened again: both there, and the picture still moves.
    w.action("save")
    w.wait("the save", lambda s: not s["modified"] and not s["saving"])
    app.quit()
    app = ctx.app([path])
    w = app.window()
    s = w.settled()
    w.maximize()
    check(s["annotations"] == 2 and not s["modified"], s)
    at = lambda x, y: w.screen(0, x, y)
    check(blue(pixel(w, "reopened", *at(400, 345))), "the saved picture isn't shown")
    inp = ctx.input().attach(w)
    inp.drag(at(400, 345), at(400, 445))
    w.wait("the saved picture moved", lambda s: s["undo"] == "Change Stamp")
    w.settled()
    check(blue(pixel(w, "file-moved", *at(400, 445))), "the saved picture didn't move")
    check(not blue(pixel(w, "file-moved-from", *at(400, 345))), "the saved picture is still drawn where it was")
    # The dialog for a new one opens, and Escape closes it.
    w.action("debug-new-signature", gvariant_str(""))
    w.wait("the dialog", lambda s: s["dialog"] == "New Signature")
    time.sleep(0.5)
    w.screenshot("new-signature")
    inp.keys(Input.ESCAPE)
    w.wait("the dialog closed", lambda s: s["dialog"] == "")


def test_text_style(ctx):
    path = os.path.join(ctx.home, "styled.pdf")
    shutil.copy(ctx.text, path)
    app = ctx.app([path])
    w = app.window()
    w.settled()
    w.maximize()
    inp = ctx.input().attach(w)
    w.action("show-markup")
    w.wait("the markup bar", lambda s: s["markup_bar"])
    time.sleep(0.4)
    at = lambda x, y: w.screen(0, x, y)
    closed = lambda s: not s["text_editor"]
    # The controls for size and font show with the text tool only.
    check(not w.state()["text_controls"], "text controls without the text tool")
    w.action("markup-tool", gvariant_str("text"))
    s = w.wait("the text controls", lambda s: s["text_controls"])
    check(s["text_style"] == "sans 12", s)
    # Set to a bold serif of 24 points: what is typed next is that. The
    # same words take more room than in the 12 points before.
    w.action("debug-text-style", gvariant_str("serif bold 24"))
    w.wait("serif bold 24", lambda s: s["text_style"] == "serif bold 24")
    inp.click(*at(100, 480))
    w.wait("the editor", lambda s: s["text_editor"])
    time.sleep(0.4)
    inp.type("big words")
    w.screenshot("typing-serif")
    inp.keys(Input.ESCAPE)
    w.wait("a text box", lambda s: closed(s) and s["annotations"] == 1)
    w.settled()
    big = dark_pixels(w, "big", at(100, 482), at(330, 440))
    check(big > 150, f"no large text on the page: {big} dark pixels")
    # Picked, a text box shows its style, and takes what is set then.
    w.action("markup-tool", gvariant_str("text"))
    s = w.wait("no tool, the box still picked", lambda s: s["markup_tool"] == "none")
    check(s["text_controls"] and s["text_style"] == "serif bold 24", s)
    w.action("debug-text-style", gvariant_str("mono regular 10"))
    s = w.wait("restyled", lambda s: s["undo"] == "Change FreeText")
    check(s["text_style"] == "mono 10" and s["annotations"] == 1, s)
    w.settled()
    small = dark_pixels(w, "small", at(100, 482), at(330, 440))
    check(small < big * 0.6, f"the text didn't get smaller: {big} then {small}")
    # Nothing picked: the controls go, and come back set to the box's
    # style when it is picked again.
    inp.keys(Input.ESCAPE)
    w.wait("the controls gone", lambda s: not s["text_controls"])
    inp.click(*at(110, 476))
    s = w.wait("the controls back", lambda s: s["text_controls"])
    check(s["text_style"] == "mono 10", s)
    # Saved and opened again, it is what it was.
    w.action("save")
    w.wait("the save", lambda s: not s["modified"] and not s["saving"])
    app.quit()
    w = ctx.app([path]).window()
    w.settled()
    w.maximize()
    inp = ctx.input().attach(w)
    inp.click(*w.screen(0, 110, 476))
    s = w.wait("the saved box picked", lambda s: s["annotation_picked"])
    check(s["text_style"] == "mono 10" and s["annotation_text"] == "big words", s)


def test_typed_text_is_kept(ctx):
    path = os.path.join(ctx.home, "typed.pdf")
    shutil.copy(ctx.text, path)
    app = ctx.app([path])
    w = app.window()
    w.settled()
    w.maximize()
    inp = ctx.input().attach(w)
    w.action("show-markup")
    w.wait("the markup bar", lambda s: s["markup_bar"])
    time.sleep(0.4)
    at = lambda x, y: w.screen(0, x, y)
    typing = lambda s: s["text_editor"]

    def typed(text):
        """Types, and waits until the text field has it all."""
        inp.type(text)
        w.wait(f"{text!r} typed", lambda s: s.get("typing_text", "").endswith(text))

    w.action("markup-tool", gvariant_str("text"))
    w.wait("the text tool", lambda s: s["markup_tool"] == "text")
    # Saving while typing: what was typed is saved, in a document that had
    # no other change.
    inp.click(*at(100, 480))
    w.wait("typing", typing)
    time.sleep(0.4)
    typed("first")
    check(not w.state()["modified"], "modified before typing ended")
    w.action("save")
    s = w.wait("saved with the text", lambda s: s["annotations"] == 1 and not s["modified"] and not s["saving"])
    check(not typing(s), s)
    # Closing while typing: the text counts as a change to ask about.
    inp.click(*at(100, 380))
    w.wait("typing", typing)
    time.sleep(0.4)
    typed("second")
    gdbus_activate(APP_PATH, "quit")
    s = w.wait("the question", lambda s: s["alert"] == "Save Changes?")
    check(s["annotations"] == 2 and not typing(s), s)
    w.action("debug-alert-response", gvariant_str("cancel"))
    w.wait("the question gone", lambda s: s["alert"] == "")
    # With the keyboard in the size field, a click on another text box:
    # what was typed stays with the box it was typed into.
    w.action("markup-tool", gvariant_str("text"))
    w.wait("no tool", lambda s: s["markup_tool"] == "none")
    inp.click(*at(110, 476), times=2)
    w.wait("typing into the first", typing)
    time.sleep(0.4)
    typed(" more")
    w.action("debug-focus-text-size", gvariant_str(""))
    time.sleep(0.3)
    check(typing(w.state()), "the size field ended typing")
    inp.click(*at(110, 376))
    s = w.wait("typing ended", lambda s: not typing(s))
    w.wait("the second picked", lambda s: s["annotation_text"] == "second")
    inp.click(*at(110, 476))
    s = w.wait("the first picked", lambda s: s["annotation_text"].startswith("first"))
    check(sorted(s["annotation_text"].split()) == ["first", "more"], s)
    # Escape from the size field ends typing too, and undo then takes the
    # typed text back, not something before it.
    inp.click(*at(110, 376), times=2)
    w.wait("typing into the second", typing)
    time.sleep(0.4)
    typed(" again")
    w.action("debug-focus-text-size", gvariant_str(""))
    time.sleep(0.3)
    inp.keys(Input.ESCAPE)
    s = w.wait("typing ended", lambda s: not typing(s) and s["annotation_text"] != "second")
    check(sorted(s["annotation_text"].split()) == ["again", "second"], s)
    w.action("undo")
    w.wait("back to what it was", lambda s: s["redo"] == "Change FreeText")
    inp.click(*at(110, 376))
    w.wait("the second as before", lambda s: s["annotation_text"] == "second")
    # Pages inserted while typing: the text is in the document first.
    inp.click(*at(110, 376), times=2)
    w.wait("typing", typing)
    time.sleep(0.4)
    typed(" kept")
    w.action("debug-insert", gvariant_str(ctx.pages))
    s = w.wait("pages inserted", lambda s: s["page_count"] > len(TEXT))
    check(not typing(s), s)
    w.action("first-page")
    w.settled("page 1", lambda s: s["page"] == 0)
    inp.click(*w.screen(0, 110, 376))
    s = w.wait("the second picked", lambda s: s["annotation_text"].startswith("second"))
    check(sorted(s["annotation_text"].split()) == ["kept", "second"], s)


def form_pdf(path):
    """One letter page with a form: a text field "name", a checkbox "agree",
    radio buttons "pick" with the choices "a" and "b", and a menu "country"
    (Germany, France and Other, held as "de", "fr" and "Other"), and a
    password field "pin". A box that is on is green."""
    def look(content):
        return (
            f"<< /Type /XObject /Subtype /Form /BBox [0 0 20 20] /Length {len(content)} >>\n"
            f"stream\n{content}\nendstream"
        ).encode()

    def button(rect, state, more):
        return (
            f"<< /Type /Annot /Subtype /Widget /Rect [{rect}] /P 4 0 R /AS /Off"
            f" /MK << /BC [0 0 0] >> /AP << /N << /{state} 10 0 R /Off 11 0 R >> >> {more} >>"
        ).encode()

    write_pdf(path, [
        b"<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [5 0 R 6 0 R 7 0 R 12 0 R 13 0 R]"
        b" /DA (/Helv 0 Tf 0 g) /DR << /Font << /Helv 3 0 R >> >> >> >>",
        b"<< /Type /Pages /Kids [4 0 R] /Count 1 >>",
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << >>"
        b" /Annots [5 0 R 6 0 R 8 0 R 9 0 R 12 0 R 13 0 R] >>",
        b"<< /Type /Annot /Subtype /Widget /FT /Tx /T (name) /Rect [100 600 400 624]"
        b" /P 4 0 R /MK << /BC [0 0 0] >> >>",
        button("100 540 120 560", "Yes", "/FT /Btn /T (agree) /V /Off"),
        b"<< /FT /Btn /Ff 32768 /T (pick) /Kids [8 0 R 9 0 R] >>",
        button("100 480 120 500", "a", "/Parent 7 0 R"),
        button("140 480 160 500", "b", "/Parent 7 0 R"),
        look("0 0.8 0 rg 0 0 20 20 re f"),
        look(""),
        b"<< /Type /Annot /Subtype /Widget /FT /Ch /Ff 131072 /T (country)"
        b" /Opt [[(de) (Germany)] [(fr) (France)] (Other)] /Rect [100 420 300 444]"
        b" /P 4 0 R /MK << /BC [0 0 0] >> >>",
        b"<< /Type /Annot /Subtype /Widget /FT /Tx /Ff 8192 /T (pin) /Rect [100 360 300 384]"
        b" /P 4 0 R /MK << /BC [0 0 0] >> >>",
    ])


def green(c):
    return c[1] > 150 and c[0] < 110 and c[2] < 110


def test_fill_form(ctx):
    path = os.path.join(ctx.home, "form.pdf")
    form_pdf(path)
    app = ctx.app([path])
    w = app.window()
    w.settled()
    w.maximize()
    inp = ctx.input().attach(w)
    at = lambda x, y: w.screen(0, x, y)
    fields = lambda s: dict(map(tuple, s["fields"]))
    typing = lambda s: s["text_editor"]
    check(
        fields(w.state()) == {"name": "", "agree": "", "pick": "", "country": "", "pin": ""},
        w.state()["fields"],
    )
    # The pointer says what a click does.
    inp.move(*at(250, 612))
    w.wait("the cursor over the text field", lambda s: s["cursor"] == "text")
    inp.move(*at(110, 550))
    w.wait("the cursor over the box", lambda s: s["cursor"] == "pointer")
    inp.move(*at(300, 300))
    w.wait("the cursor over the page", lambda s: s["cursor"] == "")
    # A click ticks the box, and another unticks it.
    inp.click(*at(110, 550))
    s = w.wait("the box ticked", lambda s: fields(s)["agree"] == "on")
    check(s["modified"] and s["undo"] == "FillField", s)
    inp.move(*at(300, 300))
    w.settled()
    check(green(pixel(w, "ticked", *at(110, 550))), "the tick isn't shown")
    inp.click(*at(110, 550))
    w.wait("the box unticked", lambda s: fields(s)["agree"] == "")
    inp.move(*at(300, 300))
    w.settled()
    check(not green(pixel(w, "unticked", *at(110, 550))), "the tick is still shown")
    w.action("undo")
    w.wait("ticked again", lambda s: fields(s)["agree"] == "on")
    # One radio button, then the other.
    inp.click(*at(110, 490))
    w.wait("a chosen", lambda s: fields(s)["pick"] == "a")
    inp.click(*at(150, 490))
    w.wait("b chosen", lambda s: fields(s)["pick"] == "b")
    inp.move(*at(300, 300))
    w.settled()
    name = w.screenshot("radio")
    width, height, pixels = read_ppm(name)
    def shot(x, y):
        k = (round(y * ctx.scale) * width + round(x * ctx.scale)) * 3
        return tuple(pixels[k:k + 3])
    check(green(shot(*at(150, 490))) and not green(shot(*at(110, 490))), "the wrong button is on")
    # The text field is typed into where it is, as wide and as high as it
    # is, and Enter ends that.
    inp.click(*at(250, 612))
    s = w.wait("typing", typing)
    time.sleep(0.4)
    x, y, width, _, height = w.wait("the field placed", lambda s: s["typing"][3] > 1)["typing"]
    zoom = s["zoom"]
    check(abs(width - 300 * zoom) <= 2 and abs(height - 24 * zoom) <= 2, s["typing"])
    check(not s["text_controls"], "the size and font controls show for a field")
    inp.type("ada")
    w.wait("typed", lambda s: s.get("typing_text") == "ada")
    inp.keys(Input.ENTER)
    s = w.wait("the name", lambda s: fields(s)["name"] == "ada")
    check(not typing(s) and s["undo"] == "FillField", s)
    w.action("undo")
    w.wait("the name undone", lambda s: fields(s)["name"] == "")
    w.action("redo")
    w.wait("the name again", lambda s: fields(s)["name"] == "ada")
    # The keyboard is on the field clicked last. Tab goes to the next one,
    # and Space ticks it.
    focus = lambda name: lambda s: s["field_focus"] == name and s["focus"] == "SpectoCanvas"
    w.wait("the keyboard on the name", focus("name"))
    inp.keys(Input.TAB)
    s = w.wait("the keyboard on the box", focus("agree"))
    check(not typing(s), s)
    inp.keys(Input.SPACE)
    w.wait("unticked by Space", lambda s: fields(s)["agree"] == "")
    inp.keys(Input.SPACE)
    w.wait("ticked by Space", lambda s: fields(s)["agree"] == "on")
    inp.keys(Input.TAB)
    w.wait("the keyboard on the first button", focus("pick"))
    inp.keys(Input.SPACE)
    w.wait("a chosen by Space", lambda s: fields(s)["pick"] == "a")
    inp.keys(Input.TAB)
    inp.keys(Input.ENTER)
    w.wait("b chosen by Enter", lambda s: fields(s)["pick"] == "b")
    # The menu: Space shows what it offers, and the arrow keys and Enter
    # choose.
    inp.keys(Input.TAB)
    s = w.wait("the keyboard on the menu", focus("country"))
    check(s["choices"] == [], s["choices"])
    inp.keys(Input.SPACE)
    w.wait("the choices", lambda s: s["choices"] == ["Germany", "France", "Other"])
    time.sleep(0.4)
    inp.keys(Input.DOWN)
    inp.keys(Input.ENTER)
    s = w.wait("France chosen", lambda s: fields(s)["country"] == "fr")
    w.wait("the choices gone", lambda s: s["choices"] == [] and focus("country")(s))
    # A click shows them too, and Escape leaves the field as it is.
    inp.click(*at(200, 432))
    w.wait("the choices", lambda s: s["choices"] != [])
    time.sleep(0.4)
    inp.keys(Input.ESCAPE)
    s = w.wait("the choices gone", lambda s: s["choices"] == [])
    check(fields(s)["country"] == "fr", s["fields"])
    w.wait("the keyboard on the menu", focus("country"))
    # A password field hides what is typed into it.
    hidden = lambda s: typing(s) and s.get("typing_hidden")
    inp.keys(Input.TAB)
    w.wait("typing into the password field", lambda s: hidden(s) and s["typing"][3] > 1)
    time.sleep(0.4)
    inp.type("ab")
    w.wait("typed", lambda s: s.get("typing_text") == "ab")
    # From the last field round to the first, a text field, which is typed
    # into. Shift+Tab keeps what was typed and goes back.
    inp.keys(Input.TAB)
    s = w.wait("typing into the name", lambda s: typing(s) and s.get("typing_text") == "ada")
    check(not s["typing_hidden"] and fields(s)["pin"] == "ab", s)
    time.sleep(0.4)
    inp.type("m")
    w.wait("typed", lambda s: s.get("typing_text") == "adam")
    inp.keys(Input.SHIFT, Input.TAB)
    s = w.wait("back in the password field", lambda s: hidden(s) and s.get("typing_text") == "ab")
    check(fields(s)["name"] == "adam", s["fields"])
    # Tab from a text field to the next field.
    time.sleep(0.4)
    inp.keys(Input.TAB)
    w.wait("typing into the name", lambda s: typing(s) and s.get("typing_text") == "adam")
    time.sleep(0.4)
    inp.keys(Input.BACKSPACE)
    w.wait("typed", lambda s: s.get("typing_text") == "ada")
    inp.keys(Input.TAB)
    s = w.wait("on the box", lambda s: not typing(s) and focus("agree")(s))
    check(fields(s)["name"] == "ada", s["fields"])
    # Escape lets go of the form, and Tab is the toolkit's again.
    inp.keys(Input.ESCAPE)
    w.wait("no field has the keyboard", lambda s: s["field_focus"] == "")
    # Typed on, and saved while typing.
    inp.click(*at(250, 612))
    w.wait("typing", typing)
    time.sleep(0.4)
    check(w.state()["typing_text"] == "ada", w.state())
    inp.type(" l")
    w.wait("typed", lambda s: s.get("typing_text") == "ada l")
    w.action("save")
    s = w.wait("the save", lambda s: not s["modified"] and not s["saving"])
    check(not typing(s) and fields(s)["name"] == "ada l", s)
    app.quit()
    w = ctx.app([path]).window()
    w.settled()
    w.maximize()
    s = w.state()
    saved = {"name": "ada l", "agree": "on", "pick": "b", "country": "fr", "pin": "ab"}
    check(fields(s) == saved, s["fields"])
    check(not s["modified"], s)
    check(green(pixel(w, "reopened", *w.screen(0, 110, 550))), "the saved tick isn't shown")
    # On a page shown turned, a text field asks for its text beside it.
    inp = ctx.input().attach(w)
    w.action("rotate-right")
    w.settled("the page turned", lambda s: s["edit_pages"][0][1] == 90)
    time.sleep(0.4)
    inp.click(*w.screen(0, 250, 612))
    s = w.wait("the editor", typing)
    check("typing" not in s, "typed into in place on a turned page")
    time.sleep(0.4)
    inp.type("x")
    inp.keys(Input.ENTER)
    s = w.wait("the name", lambda s: fields(s)["name"] == "ada lx")
    check(not typing(s) and s["undo"] == "FillField", s)


def choices_pdf(path):
    """One letter page with a form: a menu "city" (Berlin and Paris) that
    takes text of one's own too, and a list "langs" (English, French and
    Greek, held as "en", "fr" and "el") that takes several."""
    write_pdf(path, [
        b"<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [5 0 R 6 0 R]"
        b" /DA (/Helv 0 Tf 0 g) /DR << /Font << /Helv 3 0 R >> >> >> >>",
        b"<< /Type /Pages /Kids [4 0 R] /Count 1 >>",
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << >>"
        b" /Annots [5 0 R 6 0 R] >>",
        b"<< /Type /Annot /Subtype /Widget /FT /Ch /Ff 393216 /T (city)"
        b" /Opt [(Berlin) (Paris)] /Rect [100 600 300 624] /P 4 0 R /MK << /BC [0 0 0] >> >>",
        b"<< /Type /Annot /Subtype /Widget /FT /Ch /Ff 2097152 /T (langs) /V (fr)"
        b" /Opt [[(en) (English)] [(fr) (French)] [(el) (Greek)]] /Rect [100 500 300 560]"
        b" /P 4 0 R /MK << /BC [0 0 0] >> >>",
    ])


def test_fill_form_choices(ctx):
    path = os.path.join(ctx.home, "choices.pdf")
    choices_pdf(path)
    app = ctx.app([path])
    w = app.window()
    w.settled()
    w.maximize()
    inp = ctx.input().attach(w)
    at = lambda x, y: w.screen(0, x, y)
    fields = lambda s: dict(map(tuple, s["fields"]))
    shown = lambda s: s["choices"] == ["Berlin", "Paris"]
    gone = lambda s: s["choices"] == [] and s["focus"] == "SpectoCanvas"

    def closed(what, test=lambda s: True):
        """Waits for the choices to be gone, and for the keyboard to be back with the window:
        a key pressed just after they close can still go to them."""
        s = w.wait(what, lambda s: gone(s) and test(s))
        time.sleep(0.4)
        return s

    # Highlight Form Fields tints what can be filled in.
    plain = pixel(w, "plain", *at(280, 612))
    w.action("highlight-fields")
    w.settled()
    tinted = pixel(w, "tinted", *at(280, 612))
    check(plain == (255, 255, 255) and tinted[2] > tinted[0] > 150, (plain, tinted))
    w.action("highlight-fields")
    w.settled()
    check(pixel(w, "plain-again", *at(280, 612)) == plain, "the tint stays")
    # A menu that takes text of one's own has a line to type it, and Enter
    # keeps what was typed.
    inp.click(*at(200, 612))
    s = w.wait("the choices", lambda s: shown(s) and s.get("choice_typed") == "")
    time.sleep(0.4)
    inp.type("rome")
    w.wait("typed", lambda s: s.get("choice_typed") == "rome")
    inp.keys(Input.ENTER)
    s = closed("the city", lambda s: fields(s)["city"] == "rome")
    check(s["undo"] == "FillField", s)
    # It shows what the field holds, to go on from. Escape drops what was
    # typed.
    inp.keys(Input.SPACE)
    w.wait("the choices", lambda s: shown(s) and s.get("choice_typed") == "rome")
    time.sleep(0.4)
    inp.type("x")
    w.wait("typed", lambda s: s.get("choice_typed") == "romex")
    inp.keys(Input.ESCAPE)
    s = closed("the choices gone")
    check(fields(s)["city"] == "rome", s["fields"])
    # From the line, the arrow keys go to the options.
    inp.keys(Input.SPACE)
    w.wait("the choices", shown)
    time.sleep(0.4)
    inp.keys(Input.DOWN)
    inp.keys(Input.DOWN)
    inp.keys(Input.ENTER)
    closed("Paris chosen", lambda s: fields(s)["city"] == "Paris")
    # A save keeps what is being typed.
    inp.keys(Input.SPACE)
    w.wait("the choices", lambda s: shown(s) and s.get("choice_typed") == "Paris")
    time.sleep(0.4)
    inp.type("s")
    w.wait("typed", lambda s: s.get("choice_typed") == "Pariss")
    w.action("save")
    s = w.wait("the save", lambda s: not s["modified"] and not s["saving"])
    check(fields(s)["city"] == "Pariss" and s["choices"] == [], s)
    # A list that takes several has a tick for each row. Space ticks, and
    # what is ticked is chosen when the list closes.
    check(fields(s)["langs"] == "fr", s["fields"])
    langs = lambda ticked: lambda s: (
        s["choices"] == ["English", "French", "Greek"] and s.get("choices_ticked") == ticked
    )
    inp.click(*at(200, 530))
    w.wait("the list", langs([1]))
    time.sleep(0.4)
    inp.keys(Input.DOWN)
    inp.keys(Input.SPACE)
    s = w.wait("Greek ticked", langs([1, 2]))
    check(fields(s)["langs"] == "fr", s["fields"])
    # A click elsewhere closes it: beside the page view, which doesn't see
    # the pointer while the list shows.
    inp.click(130, 900)
    s = closed("French and Greek", lambda s: fields(s)["langs"] == "fr,el")
    check(s["undo"] == "FillField", s)
    # Escape leaves the field as it is.
    inp.click(*at(200, 530))
    w.wait("the list", langs([1, 2]))
    time.sleep(0.4)
    inp.keys(Input.SPACE)
    w.wait("French unticked", langs([2]))
    inp.keys(Input.ESCAPE)
    s = closed("the list gone")
    check(fields(s)["langs"] == "fr,el", s["fields"])
    # Reset Form empties every field in one step.
    w.action("reset-form")
    s = w.wait("the form reset", lambda s: fields(s) == {"city": "", "langs": ""})
    check(s["undo"] == "ResetForm", s)
    w.action("undo")
    w.wait("as before", lambda s: fields(s) == {"city": "Pariss", "langs": "fr,el"})
    w.action("save")
    w.wait("the save", lambda s: not s["modified"] and not s["saving"])
    app.quit()
    w = ctx.app([path]).window()
    w.settled()
    check(fields(w.state()) == {"city": "Pariss", "langs": "fr,el"}, w.state()["fields"])


def test_fill_form_comb(ctx):
    # One letter page with a comb field "code" of four cells of 60 points.
    path = os.path.join(ctx.home, "comb.pdf")
    write_pdf(path, [
        b"<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [5 0 R]"
        b" /DA (/Helv 12 Tf 0 g) /DR << /Font << /Helv 3 0 R >> >> >> >>",
        b"<< /Type /Pages /Kids [4 0 R] /Count 1 >>",
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << >>"
        b" /Annots [5 0 R] >>",
        b"<< /Type /Annot /Subtype /Widget /FT /Tx /Ff 16777216 /MaxLen 4 /T (code)"
        b" /Rect [100 600 340 630] /P 4 0 R >>",
    ])
    w = ctx.app([path]).window()
    w.settled()
    w.maximize()
    inp = ctx.input().attach(w)
    fields = lambda s: dict(map(tuple, s["fields"]))

    def letters(name):
        """Where the ink of each of the first three cells is, sideways: its
        middle, in points from the left of the cell."""
        width, _, pixels = read_ppm(w.screenshot(name))
        out = []
        for cell in range(3):
            (x0, y0), (x1, y1) = w.screen(0, 100 + 60 * cell, 628), w.screen(0, 160 + 60 * cell, 602)
            columns = [
                x for x in range(round(x0 * ctx.scale) + 2, round(x1 * ctx.scale) - 2)
                if any(
                    max(pixels[(y * width + x) * 3:(y * width + x) * 3 + 3]) < 110
                    for y in range(round(y0 * ctx.scale), round(y1 * ctx.scale))
                )
            ]
            check(columns, f"{name}: nothing in cell {cell}")
            middle = (columns[0] + columns[-1]) / 2 - x0 * ctx.scale
            out.append(middle / ((x1 - x0) * ctx.scale) * 60)
        return out

    # While typing, each letter stands in the middle of its cell, as it
    # does afterwards, and no more letters are taken than there are cells.
    inp.click(*w.screen(0, 220, 615))
    w.wait("typing", lambda s: s["text_editor"] and s["typing"][3] > 1)
    time.sleep(0.4)
    inp.type("wi1xy")
    w.wait("typed", lambda s: s.get("typing_text") == "wi1x")
    inp.keys(Input.BACKSPACE)
    w.wait("typed", lambda s: s.get("typing_text") == "wi1")
    inp.park()
    w.settled()
    typed = letters("typing")
    inp.keys(Input.ENTER)
    w.wait("the code", lambda s: fields(s)["code"] == "wi1" and not s["text_editor"])
    w.settled()
    saved = letters("typed")
    for cell, (a, b) in enumerate(zip(typed, saved)):
        check(abs(b - 30) < 1.5, f"cell {cell}: saved at {b}")
        # Not the last letter typed: the cursor stands beside it.
        if cell < 2:
            check(abs(a - b) < 1.5, f"cell {cell}: typed at {a}, saved at {b}")


def test_select_words_and_lines(ctx):
    w, inp = selection_window(ctx)
    inp.click(*w.char(0, 0, 12), times=2)
    selected(w, "brown")
    inp.click(*w.char(0, 1, 3), times=3)
    selected(w, "jumps over the lazy dog")
    # Shift+click extends by lines.
    inp.click(*w.char(0, 0, 5), shift=True)
    selected(w, "The quick brown fox\njumps over the lazy dog")
    # Double-click and drag: whole words.
    inp.new_sequence()
    inp.move(*w.char(0, 0, 5))
    inp.button(True)
    inp.button(False)
    inp.button(True)
    inp.drag(w.char(0, 0, 5), w.char(0, 0, 11), press=False)
    selected(w, "quick brown")


def test_drag_on_link_selects(ctx):
    w, inp = selection_window(ctx)
    inp.drag(w.char(0, LINK_LINE, 0), w.char(0, LINK_LINE, 6, before=False))
    s = selected(w, "Link to")
    check(s["page"] == 0, "the link was followed")


def test_copy_and_select_all(ctx):
    w, inp = selection_window(ctx)
    inp.drag(w.char(0, 0, 4), w.char(0, 0, 14, before=False))
    selected(w, "quick brown")
    inp.keys(Input.CONTROL, Input.KEY_C)
    deadline = time.monotonic() + TIMEOUT
    while (clip := inp.clipboard()) != "quick brown" and time.monotonic() < deadline:
        time.sleep(0.1)
    check(clip == "quick brown", f"clipboard: {clip!r}")
    inp.keys(Input.CONTROL, Input.KEY_A)
    everything = "\n".join(line for page in TEXT for line in page)
    selected(w, everything)
    inp.keys(Input.CONTROL, Input.KEY_C)
    while (clip := inp.clipboard()) != everything and time.monotonic() < deadline:
        time.sleep(0.1)
    check(clip == everything, f"clipboard: {clip!r}")


def test_select_across_pages(ctx):
    w, inp = selection_window(ctx)
    # Hold the drag below the window: it scrolls to page 2.
    s = w.state()
    start = w.char(0, 1, 0, state=s)
    inp.drag(start, (start[0], s["height"] + 40), release=False)
    w.wait("scrolling to page 2", lambda s: "Second page" in s["selection"])
    inp.button(False)
    s = w.state()
    check(s["selection"].startswith("jumps over the lazy dog\nLink to page two\nSecond page"),
          f"selection: {s['selection']!r}")


def test_select_rotated_page(ctx):
    w, inp = selection_window(ctx)
    w.action("last-page")
    s = w.settled("the rotated page", lambda s: s["page"] == 2)
    # The text runs downwards on screen. Drag along it.
    inp.drag(w.char(2, 0, 0, state=s), w.char(2, 0, 6, before=False, state=s))
    selected(w, "Rotated")


def search_for(w, inp, text):
    """Opens the search bar (or selects its text) and types `text`."""
    inp.keys(Input.CONTROL, Input.KEY_F)
    # Keys are only delivered once the bar has started sliding in.
    w.wait("the search bar", lambda s: s["search_ready"] and s["focus"] == "GtkText")
    inp.type(text)
    return w.wait(f"a search for {text!r}", lambda s: s["search_query"] == text)


def status(w, text):
    return w.wait(f"search status {text!r}", lambda s: s["search_status"] == text)


def test_search(ctx):
    w, inp = selection_window(ctx)
    search_for(w, inp, "the")
    # "The" and "the": case doesn't matter. The first is current.
    s = status(w, "1 of 2")
    check(s["match_page"] == 0, s)
    w.screenshot("the")
    # Enter goes on, wrapping around. Shift+Enter goes back.
    inp.keys(Input.ENTER)
    status(w, "2 of 2")
    inp.keys(Input.ENTER)
    status(w, "1 of 2")
    inp.keys(Input.SHIFT, Input.ENTER)
    status(w, "2 of 2")
    # Across a line break, on page 2: the view goes there.
    search_for(w, inp, "line break")
    s = status(w, "1 of 1")
    w.wait("page 2", lambda s: s["match_page"] == 1 and s["page"] == 1)
    # A word broken with a hyphen, and one with an accent.
    search_for(w, inp, "hyphenated")
    status(w, "1 of 1")
    search_for(w, inp, "cafe")
    status(w, "1 of 1")
    search_for(w, inp, "zebra")
    status(w, "No matches")
    # Escape closes the bar and ends the search.
    inp.keys(Input.ESCAPE)
    s = w.wait("the search to end", lambda s: not s["search_bar"])
    check("matches" not in s and s["focus"] == "SpectoCanvas", s)


def test_search_indexes_in_background(ctx):
    w = ctx.app([ctx.pages]).window()
    w.settled()
    w.maximize()
    inp = ctx.input().attach(w)
    w.action("debug-go-to", gvariant_str(LABELS[5]))
    w.settled("page 6", lambda s: s["page"] == 5)
    # "Page 1", "Page 10", "Page 11", "Page 12": the first after page 6
    # is current, found as the pages are indexed from there.
    search_for(w, inp, "page 1")
    s = status(w, "2 of 4")
    check(s["match_page"] == 9 and s["indexed"] == PAGES, s)
    w.wait("page 10", lambda s: s["page"] == 9)
    inp.keys(Input.SHIFT, Input.ENTER)
    status(w, "1 of 4")
    w.wait("page 1", lambda s: s["page"] == 0)


def rotations(s):
    """The rotation of each page, in degrees."""
    return [p[1] for p in s["edit_pages"]]


def test_rotate_and_undo(ctx):
    w = ctx.app([ctx.pages]).window()
    w.settled()
    inp = ctx.input().attach(w)
    w.action("next-page")
    w.settled("page 2", lambda s: s["page"] == 1)
    unrotated = [0] * PAGES
    check(rotations(w.state()) == unrotated, w.state()["edit_pages"])
    inp.keys(Input.CONTROL, Input.RIGHT)
    s = w.wait("page 2 rotated", lambda s: rotations(s)[1] == 90)
    check(rotations(s) == [0, 90] + [0] * (PAGES - 2), s["edit_pages"])
    check(s["modified"] and s["subtitle"].endswith("— Edited"), s["subtitle"])
    check(s["undo"] == "Rotate Page" and s["redo"] == "", s)
    # The page stays current, now landscape: its x axis runs down the screen.
    s = w.settled("the rotated page", lambda s: s["page"] == 1)
    a, b = next(p[1:3] for p in s["pages"] if p[0] == 1)
    check(abs(a) < 1e-6 and b > 0, f"page 2 isn't turned clockwise: {s['pages']}")
    w.showing(1, "rotated")
    inp.keys(Input.CONTROL, Input.LEFT)
    inp.keys(Input.CONTROL, Input.LEFT)
    w.wait("page 2 turned back past 0", lambda s: rotations(s)[1] == 270)
    # Undo all three, then redo one.
    for _ in range(3):
        inp.keys(Input.CONTROL, Input.KEY_Z)
    s = w.wait("everything undone", lambda s: rotations(s) == unrotated and s["undo"] == "")
    check(not s["modified"] and "Edited" not in s["subtitle"], s)
    check(s["redo"] == "Rotate Page", s)
    inp.keys(Input.SHIFT, Input.CONTROL, Input.KEY_Z)
    w.wait("the first rotation redone", lambda s: rotations(s)[1] == 90 and s["modified"])
    # Two pages side by side turn together.
    w.action("layout", gvariant_str("two-page"))
    w.action("debug-go-to", gvariant_str(LABELS[5]))
    # Pages 5 and 6 side by side. The left one counts as current.
    s = w.settled("pages 5 and 6", lambda s: s["page"] == 4)
    w.action("rotate-right")
    s = w.wait("a spread rotated", lambda s: sum(r == 90 for r in rotations(s)) == 3)
    check(rotations(s)[4] == 90 and rotations(s)[5] == 90, s["edit_pages"])
    # In the page entry, Ctrl+Left moves the cursor by a word instead.
    w.action("go-to-page")
    w.wait("the page entry", lambda s: s["focus"] == "GtkText")
    before = w.state()["edit_pages"]
    inp.keys(Input.CONTROL, Input.LEFT)
    time.sleep(0.5)
    check(w.state()["edit_pages"] == before, "Ctrl+Left in the page entry rotated a page")


def turned(s, page):
    """Whether `page` is shown turned by 90 or 270 degrees: its x axis runs
    vertically on screen."""
    a, b = next(p[1:3] for p in s["pages"] if p[0] == page)
    return abs(a) < 1e-6 and abs(b) > 0


def copy_of_pages(ctx, name="doc.pdf"):
    """A copy of the pages PDF that a test can change."""
    path = os.path.join(ctx.home, name)
    shutil.copy(ctx.pages, path)
    return path


def test_save(ctx):
    path = copy_of_pages(ctx)
    app = ctx.app([path])
    w = app.window()
    w.settled()
    w.action("next-page")
    w.settled("page 2", lambda s: s["page"] == 1)
    w.action("rotate-right")
    w.wait("page 2 rotated", lambda s: s["modified"])
    w.action("save")
    s = w.wait("the save", lambda s: not s["modified"] and not s["saving"])
    check("Edited" not in s["subtitle"] and s["toast"] == "", s)
    app.quit()
    # Opened again (on page 2, where it was closed): page 2 is turned,
    # nothing else, and it isn't an edit. Not "next-page": whether that
    # arrives before or after the saved position is restored is a race.
    app = ctx.app([path])
    w = app.window()
    s = w.settled("page 2", lambda s: s["page"] == 1)
    check(turned(s, 1) and not turned(s, 0), s["pages"])
    check(not s["modified"] and s["page_entry"] == LABELS[1], s)
    # Save As: a copy, shown from then on. The file opened stays as it was.
    before = open(path, "rb").read()
    w.action("rotate-left")
    w.wait("page 2 turned back", lambda s: s["modified"])
    copy = os.path.join(ctx.home, "copy.pdf")
    w.action("debug-save-as", gvariant_str(copy))
    s = w.wait("the copy", lambda s: s["title"] == "copy.pdf" and not s["modified"])
    check(open(path, "rb").read() == before, "Save As changed the original file")
    check(os.path.exists(copy), "no copy written")
    app.quit()
    # The copy took over the reading position too.
    w = ctx.app([copy]).window()
    s = w.settled("page 2 of the copy", lambda s: s["page"] == 1)
    check(not turned(s, 1), s["pages"])


def test_close_prompt(ctx):
    path = copy_of_pages(ctx)
    app = ctx.app([path])
    w = app.window()
    w.settled()
    w.action("rotate-right")
    w.wait("page 1 rotated", lambda s: s["modified"])
    # Cancel: the window stays, still edited.
    gdbus_activate(APP_PATH, "quit")
    w.wait("the question", lambda s: s["alert"] == "Save Changes?")
    time.sleep(0.5)  # the dialog's animation
    w.screenshot("prompt")
    w.action("debug-alert-response", gvariant_str("cancel"))
    s = w.wait("the question to go", lambda s: s["alert"] == "")
    time.sleep(0.5)
    check(app.proc.poll() is None and w.state()["modified"], "the window closed")
    # Save: saved, then closed.
    gdbus_activate(APP_PATH, "quit")
    w.wait("the question", lambda s: s["alert"] == "Save Changes?")
    w.action("debug-alert-response", gvariant_str("save"))
    try:
        app.proc.wait(timeout=TIMEOUT)
    except subprocess.TimeoutExpired:
        raise CheckFailed("the app didn't quit after saving")
    check(app.proc.returncode == 0, f"app exited with {app.proc.returncode}")
    # Discard: closed without saving the second rotation.
    app = ctx.app([path])
    w = app.window()
    s = w.settled()
    check(turned(s, 0), "the first rotation wasn't saved")
    w.action("rotate-right")
    w.wait("page 1 rotated again", lambda s: s["modified"])
    gdbus_activate(APP_PATH, "quit")
    w.wait("the question", lambda s: s["alert"] == "Save Changes?")
    w.action("debug-alert-response", gvariant_str("discard"))
    try:
        app.proc.wait(timeout=TIMEOUT)
    except subprocess.TimeoutExpired:
        raise CheckFailed("the app didn't quit after discarding")
    s = ctx.app([path]).window().settled()
    check(turned(s, 0), "the discarded rotation was saved")


def click_thumbnail(w, inp, page, **keys):
    """Clicks the middle of `page`'s thumbnail (on screen in the sidebar)."""
    s = w.state()
    rect = next((t[1:] for t in s["thumbnails"] if t[0] == page), None)
    check(rect is not None, f"no thumbnail of page {page} on screen: {s['thumbnails']}")
    x, y, width, height = rect
    # From a place on the canvas, where the pointer's position is checked,
    # so the move into the sidebar starts from a known point.
    inp.move(s["width"] * 0.75, s["height"] / 2)
    inp.click(x + width / 2, y + height / 2, **keys)


def selected_thumbnails(w, pages):
    return w.wait(
        f"thumbnails {pages} selected", lambda s: s.get("thumbnails_selected") == pages
    )


def test_thumbnail_selection(ctx):
    w = ctx.app([ctx.pages]).window()
    w.settled()
    w.maximize()
    s = w.settled("maximized")
    inp = ctx.input().attach(w)
    check(s["thumbnails_selected"] == [0], s)
    # A click goes to the page.
    click_thumbnail(w, inp, 1)
    selected_thumbnails(w, [1])
    w.settled("page 2", lambda s: s["page"] == 1)
    # Shift+click and Ctrl+click select more, without going there. (Only
    # three thumbnails fit at scale 2.)
    click_thumbnail(w, inp, 2, shift=True)
    selected_thumbnails(w, [1, 2])
    click_thumbnail(w, inp, 0, control=True)
    selected_thumbnails(w, [0, 1, 2])
    click_thumbnail(w, inp, 1, control=True)
    s = selected_thumbnails(w, [0, 2])
    check(s["page"] == 1, f"the view moved to page {s['page'] + 1}")
    # Rotate turns the selected pages, which stay selected.
    w.action("rotate-right")
    s = w.wait("the selected pages rotated", lambda s: rotations(s)[:3] == [90, 0, 90])
    check(sum(rotations(s)) == 180 and s["thumbnails_selected"] == [0, 2], s)
    # Moving the view keeps the selection.
    w.action("last-page")
    w.settled("the last page", lambda s: s["page"] == PAGES - 1)
    check(w.state()["thumbnails_selected"] == [0, 2], "the selection didn't stay")
    w.action("first-page")
    w.settled("page 1", lambda s: s["page"] == 0)
    # A plain click selects one again. Then Rotate turns the current page.
    click_thumbnail(w, inp, 1)
    selected_thumbnails(w, [1])
    w.settled("page 2", lambda s: s["page"] == 1)
    w.action("rotate-right")
    s = w.wait("page 2 rotated", lambda s: rotations(s)[1] == 90)
    check(sum(rotations(s)) == 270, s["edit_pages"])
    # With the sidebar focused, Ctrl+A selects every page, not the text.
    # Rarely (2 in about 40 runs) neither ever happens, as if the key never
    # arrived, so it gets a second press then.
    w.settled()
    everything = list(range(PAGES))
    inp.keys(Input.CONTROL, Input.KEY_A)
    try:
        s = w.wait("every thumbnail selected", lambda s: s["thumbnails_selected"] == everything, 2)
    except CheckFailed:
        check(w.state()["selection"] == "", "Ctrl+A selected the text")
        inp.keys(Input.CONTROL, Input.KEY_A)
        s = selected_thumbnails(w, everything)
    check(s["selection"] == "", f"text selected: {s['selection']!r}")
    # Undo: the selection goes back to following the current page.
    w.action("undo")
    w.wait("page 2 turned back", lambda s: rotations(s)[1] == 0)
    selected_thumbnails(w, [w.state()["page"]])


def file_pages(s):
    """The page of the file at each position."""
    return [p[0] for p in s["edit_pages"]]


def test_delete_pages(ctx):
    w = ctx.app([ctx.pages]).window()
    w.settled()
    w.maximize()
    w.settled("maximized")
    inp = ctx.input().attach(w)
    w.action("debug-go-to", gvariant_str(LABELS[2]))
    w.settled("page 3", lambda s: s["page"] == 2)
    # The document has the focus: Delete and Backspace delete no page, and
    # Backspace scrolls back.
    s = w.wait("the canvas focused", lambda s: s["focus"] == "SpectoCanvas")
    top = s["pages"][0][6]
    inp.keys(Input.DELETE)
    inp.keys(Input.BACKSPACE)
    s = w.wait("scrolled back", lambda s: s["pages"] and s["pages"][0][6] != top)
    time.sleep(0.3)
    check(len(w.state()["edit_pages"]) == PAGES and s["undo"] == "", s)
    w.action("debug-go-to", gvariant_str(LABELS[2]))
    w.settled("page 3", lambda s: s["page"] == 2)
    # In the sidebar, Delete deletes the current page, and the next one
    # takes its place.
    click_thumbnail(w, inp, 2)
    selected_thumbnails(w, [2])
    inp.keys(Input.DELETE)
    s = w.wait("page 3 deleted", lambda s: len(s["edit_pages"]) == PAGES - 1)
    check(file_pages(s) == [0, 1] + list(range(3, PAGES)), s["edit_pages"])
    check(s["undo"] == "Delete Page" and s["subtitle"].startswith("11 pages"), s)
    s = w.settled("the page after it", lambda s: s["page"] == 2)
    w.showing(3, "after-delete")
    # It keeps its label.
    check(s["page_entry"] == LABELS[3], s)
    # Two selected in the sidebar: Backspace deletes both.
    w.action("first-page")
    w.settled("page 1", lambda s: s["page"] == 0 and s["thumbnails"][0][0] == 0)
    click_thumbnail(w, inp, 0)
    click_thumbnail(w, inp, 1, control=True)
    selected_thumbnails(w, [0, 1])
    inp.keys(Input.BACKSPACE)
    s = w.wait("pages 1 and 2 deleted", lambda s: len(s["edit_pages"]) == PAGES - 3)
    check(file_pages(s) == list(range(3, PAGES)), s["edit_pages"])
    check(s["undo"] == "Delete 2 Pages", s)
    # In the page entry, Backspace edits the text.
    w.action("go-to-page")
    w.wait("the page entry", lambda s: s["focus"] == "GtkText")
    inp.keys(Input.BACKSPACE)
    time.sleep(0.5)
    check(len(w.state()["edit_pages"]) == PAGES - 3, "Backspace in the page entry deleted a page")
    # Every page selected: nothing is deleted (a document keeps a page).
    click_thumbnail(w, inp, 0)
    inp.keys(Input.CONTROL, Input.KEY_A)
    selected_thumbnails(w, list(range(PAGES - 3)))
    inp.keys(Input.DELETE)
    time.sleep(0.5)
    check(len(w.state()["edit_pages"]) == PAGES - 3, "every page deleted")
    # Undo brings them back.
    w.action("undo")
    w.action("undo")
    s = w.wait("both undone", lambda s: len(s["edit_pages"]) == PAGES)
    check(not s["modified"], s)


def drop_thumbnails(w, pages, onto, lower=True):
    """Drops `pages` on the lower (or upper) half of `onto`'s thumbnail.
    Through a test hook that does what a drop does: Mutter refuses about
    one drag in three from the virtual pointer (GTK cancels it at once),
    so real drags would make the check flaky."""
    s = w.state()
    rect = next((t[1:] for t in s["thumbnails"] if t[0] == onto), None)
    check(rect is not None, f"no thumbnail of page {onto} on screen: {s['thumbnails']}")
    _, y, _, height = rect
    # Just off the middle: the lower part may be below the window's edge.
    at = y + height * (0.6 if lower else 0.4)
    check(at < s["height"], f"page {onto}'s thumbnail is too far down: {rect}")
    w.action("debug-drop-thumbnails", gvariant_str(f"{','.join(map(str, pages))}@{at}"))


def test_move_pages(ctx):
    w = ctx.app([ctx.pages]).window()
    w.settled()
    w.maximize()
    w.settled("maximized")
    # Page 1 dropped on the lower half of page 3: it goes after it (only
    # three thumbnails fit at scale 2).
    drop_thumbnails(w, [0], 2)
    w.wait("page 1 moved", lambda s: file_pages(s)[:4] == [1, 2, 0, 3])
    # The view stays on the page it showed, now the third.
    s = w.settled("page 1 in its new place", lambda s: s["page"] == 2)
    check(s["undo"] == "Move Page" and s["modified"], s)
    check(s["thumbnails_selected"] == [2], s)
    w.showing(0, "moved")
    # Two pages (2 and 3, now first) dropped on the lower half of the
    # third: they go after page 1, where they were.
    w.action("first-page")
    w.settled("the first page", lambda s: s["page"] == 0 and s["thumbnails"][0][0] == 0)
    drop_thumbnails(w, [0, 1], 2)
    w.wait("both moved", lambda s: file_pages(s) == list(range(PAGES)))
    s = w.settled()
    check(s["undo"] == "Move 2 Pages" and s["thumbnails_selected"] == [1, 2], s)
    # Dropped where they are: nothing changes.
    drop_thumbnails(w, [1, 2], 1, lower=False)
    time.sleep(0.5)
    check(w.state()["undo"] == "Move 2 Pages", "a drop in place was an edit")
    # Undone, the order is as before the second move.
    w.action("undo")
    w.wait("the move undone", lambda s: file_pages(s)[:4] == [1, 2, 0, 3])


def test_drop_files_and_export(ctx):
    path = copy_of_pages(ctx)
    w = ctx.app([path]).window()
    w.settled()
    w.maximize()
    s = w.settled("maximized")
    # Two copies of the three-page text PDF, dropped below page 1.
    rect = next(t[1:] for t in s["thumbnails"] if t[0] == 0)
    at = rect[1] + rect[3] * 0.6
    w.action("debug-drop-files", gvariant_str(f"{ctx.text}|{ctx.text}@{at}"))
    s = w.wait("the files inserted", lambda s: len(s["edit_pages"]) == PAGES + 6)
    check([p[2] for p in s["edit_pages"][:8]] == [0, 1, 1, 1, 2, 2, 2, 0], s["edit_pages"])
    check(s["undo"] == "Insert 6 Pages", s)
    s = w.settled("the first inserted page", lambda s: s["page"] == 1)
    check(s["thumbnails_selected"] == list(range(1, 7)), s)
    # The selected pages exported: a new file with just those.
    out = os.path.join(ctx.home, "export.pdf")
    w.action("debug-export", gvariant_str(out))
    s = w.wait("the export", lambda s: s["toast"] == "Exported 6 pages to “export.pdf”")
    check(s["title"] == "doc.pdf" and s["modified"], s)
    # pdfinfo (poppler-utils) reads it independently of Specto.
    info = subprocess.run(["pdfinfo", out], capture_output=True, text=True)
    check("Pages:           6" in info.stdout, f"pdfinfo: {info.stdout}{info.stderr}")


def test_pages_between_windows(ctx):
    path = os.path.join(ctx.home, "text.pdf")
    shutil.copy(ctx.text, path)
    app = ctx.app([ctx.pages, path])
    one, two = app.window(1), app.window(2)
    one.wait("the document", lambda s: s["view"] == "document")
    # Its thumbnails, not its render: only the window on top is sure to draw
    # (see two_windows), and which one that is isn't sure either.
    s = two.wait(
        "the second document",
        lambda s: s["view"] == "document" and any(t[0] == 0 for t in s["thumbnails"]),
    )
    one.action("debug-go-to", gvariant_str(LABELS[2]))
    one.wait("page 3", lambda s: s["page"] == 2)
    one.action("rotate-right")
    one.wait("page 3 rotated", lambda s: rotations(s)[2] == 90)
    # Pages 1 and 3 of the first window, dropped below page 1 of the second.
    rect = next(t[1:] for t in s["thumbnails"] if t[0] == 0)
    at = rect[1] + rect[3] * 0.6
    two.action("debug-drop-pages-from", gvariant_str(f"1:0,2@{at}"))
    s = two.wait("the pages copied", lambda s: len(s["edit_pages"]) == 5)
    check([(p[0], p[2]) for p in s["edit_pages"]] == [(0, 0), (0, 1), (2, 1), (1, 0), (2, 0)],
          s["edit_pages"])
    check(rotations(s) == [0, 0, 90, 0, 0], s["edit_pages"])
    check(s["undo"] == "Insert 2 Pages" and s["thumbnails_selected"] == [1, 2], s)
    # Its page, not its render: at scale 2 the windows overlap and the
    # compositor may stop sending this one frames.
    two.wait("the first copied page", lambda s: s["page"] == 1)
    # The first window still has them.
    check(len(one.state()["edit_pages"]) == PAGES, "pages left the first window")
    # Saved, the second file has them.
    two.action("save")
    two.wait("the save", lambda s: not s["modified"] and not s["saving"])
    info = subprocess.run(["pdfinfo", path], capture_output=True, text=True)
    check("Pages:           5" in info.stdout, f"pdfinfo: {info.stdout}{info.stderr}")


def replace_file(src, dst):
    """Replaces `dst` with a copy of `src` the way editors do: written
    next to it, then renamed over it."""
    tmp = dst + ".tmp"
    shutil.copy(src, tmp)
    os.replace(tmp, dst)


def test_file_changed(ctx):
    path = copy_of_pages(ctx)
    w = ctx.app([path]).window()
    w.settled()
    w.action("debug-go-to", gvariant_str(LABELS[2]))
    w.settled("page 3", lambda s: s["page"] == 2)
    # No edits: the new file is shown, at the same place.
    replace_file(ctx.text, path)
    s = w.wait("the reload", lambda s: len(s["edit_pages"]) == 3)
    check(s["title"] == "doc.pdf" and not s["modified"] and s["banner"] == "", s)
    w.wait("the same page", lambda s: s["page"] == 2)
    # Our own save doesn't count as a change: the edit history stays.
    w.action("rotate-right")
    w.wait("a page rotated", lambda s: s["modified"])
    w.action("save")
    w.wait("the save", lambda s: not s["modified"] and not s["saving"])
    time.sleep(1)
    s = w.state()
    check(s["undo"] == "Rotate Page" and s["banner"] == "", f"reloaded after saving: {s}")
    # With edits: a banner, and the edits stay.
    w.action("rotate-right")
    w.wait("another rotation", lambda s: s["modified"])
    replace_file(ctx.pages, path)
    s = w.wait("the banner", lambda s: "changed by another program" in s["banner"])
    check(len(s["edit_pages"]) == 3 and s["modified"], s)
    # Saving asks first. Cancel leaves the other program's file.
    w.action("save")
    w.wait("the question", lambda s: s["alert"] == "File Changed on Disk")
    w.action("debug-alert-response", gvariant_str("cancel"))
    w.wait("the question to go", lambda s: s["alert"] == "")
    check(open(path, "rb").read() == open(ctx.pages, "rb").read(), "the file was overwritten")
    # Reload: the other program's version, edits gone.
    w.action("reload")
    s = w.wait("the reload", lambda s: len(s["edit_pages"]) == PAGES)
    check(not s["modified"] and s["banner"] == "" and s["undo"] == "", s)
    # Deleted: a banner. Save writes the file again.
    os.remove(path)
    w.wait("the banner", lambda s: "moved or deleted" in s["banner"])
    w.action("save")
    w.wait("the file written again", lambda s: os.path.exists(path) and s["banner"] == "")


def printed_pages(path):
    """Each page of the PDF at `path`: (landscape, color covering most of
    it), read with poppler (pdfinfo, pdftoppm) rather than Specto."""
    info = subprocess.run(["pdfinfo", "-f", "1", "-l", "999", path],
                          capture_output=True, text=True).stdout
    sizes = re.findall(r"^Page +\d+ size: +([\d.]+) x ([\d.]+)", info, re.M)
    out = []
    for i, (w, h) in enumerate(sizes, 1):
        prefix = os.path.join(os.path.dirname(path), f"printed-{i}")
        subprocess.run(["pdftoppm", "-r", "20", "-f", str(i), "-l", str(i),
                        "-singlefile", path, prefix], check=True)
        # Printing and reading back may move a channel by one or two.
        width, height, pixels = read_ppm(prefix + ".ppm")
        counts = {}
        for k in range(0, len(pixels), 3):
            px = pixels[k : k + 3]
            for page, c in enumerate(PALETTE[:PAGES]):
                if all(abs(a - b) <= 3 for a, b in zip(px, c)):
                    counts[page] = counts.get(page, 0) + 1
                    break
        top = max(counts, key=counts.get) if counts else None
        out.append((float(w) > float(h), top))
    return out


def test_print_to_file(ctx):
    w = ctx.app([ctx.pages]).window()
    w.settled()
    w.maximize()
    w.settled("maximized")
    inp = ctx.input().attach(w)
    w.action("next-page")
    w.wait("page 2", lambda s: s["page"] == 1)
    w.action("rotate-right")
    w.wait("page 2 rotated", lambda s: rotations(s)[1] == 90)
    out = os.path.join(ctx.home, "printed.pdf")
    w.action("debug-print-to-file", gvariant_str(out))
    s = w.wait("the print", lambda s: s["printed"] == 1)
    check(s["toast"] == "", s)
    pages = printed_pages(out)
    check(len(pages) == PAGES, f"{len(pages)} pages printed")
    # Page 2 turned by the user and page ROTATED by its file are landscape.
    landscape = [i for i, (wide, _) in enumerate(pages) if wide]
    check(landscape == sorted([1, ROTATED]), f"landscape: {landscape}")
    check([c for _, c in pages] == list(range(PAGES)), f"colours: {pages}")
    # Selection: thumbnails 1 and 3 only.
    click_thumbnail(w, inp, 0)
    click_thumbnail(w, inp, 2, control=True)
    selected_thumbnails(w, [0, 2])
    out = os.path.join(ctx.home, "selection.pdf")
    w.action("debug-print-to-file", gvariant_str(out + "|selection"))
    w.wait("the second print", lambda s: s["printed"] == 2)
    pages = printed_pages(out)
    check([c for _, c in pages] == [0, 2], f"selection printed: {pages}")


def test_insert_file(ctx):
    path = copy_of_pages(ctx)
    app = ctx.app([path])
    w = app.window()
    w.settled()
    inp = ctx.input().attach(w)
    w.action("next-page")
    w.settled("page 2", lambda s: s["page"] == 1)
    # The text PDF's three pages go after page 2, and the view goes there.
    w.action("debug-insert", gvariant_str(ctx.text))
    s = w.wait("the inserted pages", lambda s: len(s["edit_pages"]) == PAGES + 3)
    inserted = [[0, 0, 1], [1, 0, 1], [2, 0, 1]]
    check(s["edit_pages"][2:5] == inserted, s["edit_pages"])
    check(s["edit_pages"][5] == [2, 0, 0], s["edit_pages"])
    check(s["undo"] == "Insert 3 Pages" and s["modified"], s)
    s = w.settled("the first inserted page", lambda s: s["page"] == 2)
    # The inserted pages have no labels: numbered by position. The others
    # keep theirs.
    check(s["page_entry"] == "3" and s["page_total"] == "(3 of 15)", s)
    check(s["subtitle"].startswith("15 pages"), s)
    w.action("debug-go-to", gvariant_str(LABELS[2]))
    w.wait("page iii, now sixth", lambda s: s["page"] == 5)
    w.action("debug-go-to", gvariant_str("3"))
    w.wait("the first inserted page", lambda s: s["page"] == 2)
    # (Original page 3's thumbnail shows in the sidebar, but not the page.)
    c = w.colours("inserted")
    check(c.get(2, 0) < 0.1, f"page 3 of the original still shown: {fmt(c)}")
    # Searching finds text on the inserted pages, at their new positions.
    search_for(w, inp, "lazy")
    s = status(w, "1 of 1")
    check(s["match_page"] == 2, s)
    inp.keys(Input.ESCAPE)
    # Undone, and done again.
    w.action("undo")
    s = w.wait("the insert undone", lambda s: len(s["edit_pages"]) == PAGES)
    check(not s["modified"] and s["page_entry"] == LABELS[s["page"]], s)
    w.action("redo")
    w.wait("the insert redone", lambda s: len(s["edit_pages"]) == PAGES + 3)
    w.action("save")
    w.wait("the save", lambda s: not s["modified"] and not s["saving"])
    app.quit()
    # One file now: the inserted pages are its pages 3 to 5.
    w = ctx.app([path]).window()
    s = w.settled()
    check([p[2] for p in s["edit_pages"]] == [0] * (PAGES + 3), s["edit_pages"])
    w.action("debug-go-to", gvariant_str("4"))
    w.settled("page 4", lambda s: s["page"] == 3)
    search_for(w, inp.attach(w), "Second page")
    s = status(w, "1 of 1")
    check(s["match_page"] == 3, s)


def test_select_after_rotating(ctx):
    w, inp = selection_window(ctx)
    inp.keys(Input.CONTROL, Input.RIGHT)
    w.wait("page 1 rotated", lambda s: rotations(s)[0] == 90)
    s = w.stable()
    # The text runs downwards on screen now. Drag along it.
    inp.drag(w.char(0, 0, 4, state=s), w.char(0, 0, 14, before=False, state=s))
    selected(w, "quick brown")
    # Search highlights still find their page.
    search_for(w, inp, "lazy")
    s = status(w, "1 of 1")
    check(s["match_page"] == 0, s)
    w.screenshot("rotated-search")


TESTS = [v for k, v in list(globals().items()) if k.startswith("test_")]


# Runner


class Context:
    def __init__(self, binary, tmp, out):
        self.binary, self.tmp, self.out = binary, tmp, out
        self.scale = float(os.environ.get("SPECTO_SCALE", "1"))
        self.pages = os.path.join(tmp, "pages.pdf")
        self.broken = os.path.join(tmp, "broken.pdf")
        self.text = os.path.join(tmp, "text.pdf")
        text_pdf(self.text)
        self._input = None
        pages_pdf(self.pages)
        with open(self.broken, "wb") as f:
            f.write(b"%PDF-1.7\nthis is not a PDF\n")
        self.apps = []
        self.test = ""

    def fresh_env(self):
        """Fresh saved state and recent files for each test."""
        home = tempfile.mkdtemp(prefix=f"{self.test}-", dir=self.tmp)
        self.home = home
        self.env = dict(os.environ, SPECTO_DEBUG="1")
        for var in ("XDG_STATE_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME"):
            self.env[var] = os.path.join(home, var.lower())

    def input(self):
        """The virtual pointer and keyboard (one session for the whole run)."""
        if self._input is None:
            self._input = Input()
        return self._input

    def app(self, files=()):
        a = App(self, files)
        self.apps.append(a)
        return a

    def cleanup(self):
        for a in self.apps:
            a.kill()
        self.apps = []
        # The test's state and caches (shader caches add up). Screenshots
        # and the log are elsewhere.
        shutil.rmtree(self.home, ignore_errors=True)


def main():
    args = sys.argv[1:]
    binary = os.path.abspath(args.pop(0) if args and os.sep in args[0] else "target/debug/specto-gtk")
    tests = [t for t in TESTS if not args or t.__name__.removeprefix("test_") in args]
    tmp = tempfile.mkdtemp(prefix="specto-check-")
    open(os.path.join(tmp, "app.log"), "w").close()
    out = os.environ.get("SPECTO_CHECK_OUT") or os.path.join(tmp, "out")
    os.makedirs(out, exist_ok=True)
    ctx = Context(binary, tmp, out)
    failed = []
    for t in tests:
        ctx.test = t.__name__.removeprefix("test_")
        ctx.fresh_env()
        start = time.monotonic()
        try:
            t(ctx)
            for a in ctx.apps:
                if a.proc.poll() not in (None, 0):
                    raise CheckFailed(f"app exited with {a.proc.returncode}")
            print(f"ok    {ctx.test} ({time.monotonic() - start:.1f} s)", flush=True)
        except Exception as e:
            failed.append(ctx.test)
            detail = str(e) if isinstance(e, CheckFailed) else traceback.format_exc()
            print(f"FAIL  {ctx.test}: {detail}", flush=True)
        finally:
            ctx.cleanup()
    with open(os.path.join(tmp, "app.log")) as f:
        log = f.read().strip()
    if failed:
        if log:
            print(f"\napp output:\n{log}")
        print(f"\n{len(failed)} of {len(tests)} failed: {' '.join(failed)}")
        print(f"screenshots: {out}")
        sys.exit(1)
    print(f"\nall {len(tests)} passed (scale {ctx.scale})")
    shutil.rmtree(tmp)


if __name__ == "__main__":
    main()
