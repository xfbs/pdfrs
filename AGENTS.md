# Rules for agents

## Prose (documentation and comments)

- No semicolons. Split into sentences, or join with "and".
- No glyphs: no "…", arrows, check marks, "×", "≤" or curly quotes. Write "and
  so on", "becomes", "by", "at most". Exception: "√2" for the zoom levels.
- No abbreviations: write "such as" for "e.g.", "and" for "+", "pixels" for
  "px", "specification" for "spec", "right-to-left" for "RTL".
- No slash for "and" or "or".
- US spelling: color, center, gray, behavior, license, neighbor.
- Ranges and sizes in words: "3 to 5", "600 by 800 points".
- Say what exists. No milestone references ("M3"), no history ("this used to"),
  no plans in the present tense.
- Be concise. Cut what the reader doesn't need.
- No banner comments: no rules of dashes or other characters around or
  between comments. Where a long file needs a heading for a group of items, a
  plain comment of a word or two does it, such as `// Search`.

## Markdown files

- Wrap at 80 columns.
- Inline code only for paths, file names and identifiers a reader would type.
  Crate and library names (hayro, lopdf, specto-view) are plain words.
- Commands go in fenced code blocks, never inline in a paragraph. Comments go
  on their own line above the command.
- No tables with long cells. Use a list.
- Feature lists in READMEs: a bold one-word label per bullet, lowercase after
  the colon.
- Don't repeat what another file says. Link to it. Crate descriptions live in
  `crates/README.md`.
- "Project structure" sections list only what isn't obvious from file names.
- The Android three-dot menu is "the overflow menu".

## Rust comments

- Wrap at 100 columns.
- Doc comments: a one-sentence summary, a blank `///` line, then the detail.
  A very short second sentence may stay with the summary.
- The summary says what the item is or does, not background.
- Backticks on identifiers in doc comments. Link functions as `` [`name()`] ``
  when a macro has the same name.
- A blank line above every top-level doc comment.

## Kotlin comments

- Wrap code and comments at 100 columns.
- Format with ktlint after a change. Run it from `apps/android`:

  ```sh
  ./gradlew ktlintFormat
  ```

- KDoc (`/** ... */`) on declarations, `//` inside functions.
- KDoc: a one-sentence summary, a blank ` *` line, then the detail. A KDoc that
  fits on one line stays on one line.
- Describe parameters in the prose with `[name]` links. No `@param` or
  `@return` tags unless they add something.

## Keeping documents current

- A change to a feature updates the app's `FEATURES.md` in the same commit,
  and `TESTING.md` where a manual check changes.
- A change that users notice adds an item under "Unreleased" in `CHANGELOG.md`
  in the same commit, under the app it is in, or "Both apps". Say what they
  can do or see now, not how it was built. Fixes count when users ran into
  the problem.
- `docs/TASKS.md`: within a milestone, done tasks first in the order they were
  done, then open ones. Done tasks are one line. No sizes.
- `docs/ARCHITECTURE.md` describes what exists. Planned work goes in its
  "Planned" section.
- "Evaluation", not "spike".

## Git

- Never create commits. Suggest a commit message instead.
