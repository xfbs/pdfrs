#!/bin/sh
# Collects the desktop app's translatable strings into po/specto.pot, the
# template translations start from. Run it after changing strings.
#
#     apps/desktop/po/update-pot.sh
#
# With --check it writes nothing, and fails if the template isn't what it
# would write. CI runs it that way.
#
# The translations (po/*.po) are left alone. They come from Crowdin, which
# brings them up to date with the template.
#
# Needs xgettext from GNU gettext 0.24 or newer (the first that reads Rust).
set -eu
here=$(cd "$(dirname "$0")" && pwd)
cd "$here/.."
check=false
if [ "${1:-}" = --check ]; then
    check=true
fi
out=$(mktemp)
trap 'rm -f "$out"' EXIT
# The functions of src/i18n.rs, in every source file. Plain file order, so the template doesn't
# change with the locale of whoever runs this.
xgettext --language=Rust --from-code=UTF-8 --add-comments=Translators: --no-git \
    --keyword=gettext --keyword=gettext_f \
    --keyword=ngettext_f:1,2 \
    --package-name=specto --msgid-bugs-address=https://github.com/xfbs/specto/issues \
    --copyright-holder="The Specto contributors" \
    --output="$out" \
    $(find src -name '*.rs' | LC_ALL=C sort)
# Without the creation date, running this again changes nothing unless a
# string did.
sed -i.bak '/^"POT-Creation-Date: /d' "$out" && rm "$out.bak"
if $check; then
    if ! diff -u po/specto.pot "$out"; then
        echo "$here/specto.pot is out of date. Run apps/desktop/po/update-pot.sh." >&2
        exit 1
    fi
    exit 0
fi
cp "$out" po/specto.pot
echo "$here/specto.pot: $(grep -c '^msgid ' po/specto.pot) strings"
