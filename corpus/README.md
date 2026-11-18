# Test corpus

The corpus is the set of test PDFs from pdf.js, about 1,100 files and 150 MB.
It is not committed. Download it into `corpus/files/` with:

```sh
corpus/fetch.sh
```

Then run the rendering regression, from the repository root:

```sh
cargo build --release -p specto-cli
target/release/specto corpus corpus/files/pdfjs/test/pdfs --reference --max-pages 3
```

This renders the first three pages of every file and compares them with
Poppler's rendering (pdftoppm). It writes a report and the results as a table
to `out/corpus/`, and side-by-side images of the pages that differ to
`out/corpus/diffs/`.

Poppler is not ground truth, and about half of the worst differences are
Poppler bugs. Treat a large difference as a page to look at, not as proof that
Specto renders it wrong.
