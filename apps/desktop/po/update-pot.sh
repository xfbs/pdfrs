#!/bin/sh
# Collects the desktop app's translatable strings into po/specto.pot, the
# template translations start from, and brings the translations (po/*.po) up
# to date with it. Run it after changing strings.
#
#     apps/desktop/po/update-pot.sh
#
# Needs xgettext from GNU gettext 0.24 or newer (the first that reads Rust).
set -eu
here=$(cd "$(dirname "$0")" && pwd)
cd "$here/.."
# The functions of src/i18n.rs, in every source file. Plain file order, so the template doesn't
# change with the locale of whoever runs this.
xgettext --language=Rust --from-code=UTF-8 --add-comments=Translators: --no-git \
    --keyword=gettext --keyword=gettext_f \
    --keyword=ngettext_f:1,2 \
    --package-name=specto --msgid-bugs-address=https://github.com/xfbs/specto/issues \
    --copyright-holder="The Specto contributors" \
    --output=po/specto.pot \
    $(find src -name '*.rs' | LC_ALL=C sort)
# Without the creation date, running this again changes nothing unless a
# string did.
sed -i.bak '/^"POT-Creation-Date: /d' po/specto.pot && rm po/specto.pot.bak
echo "$here/specto.pot: $(grep -c '^msgid ' po/specto.pot) strings"
# The translations follow the template: a new string is added empty, one that
# is gone is dropped. Nothing is guessed from similar strings, which would
# have to be looked over by someone who reads the language.
for po in po/*.po; do
    [ -e "$po" ] || continue
    msgmerge --quiet --update --backup=none --no-fuzzy-matching "$po" po/specto.pot
    msgattrib --no-obsolete --output-file="$po" "$po"
    echo "$here/$(basename "$po"): $(msgfmt --statistics -o /dev/null "$po" 2>&1)"
done
