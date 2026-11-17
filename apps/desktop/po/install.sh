#!/bin/sh
# Compiles the desktop app's translations (po/*.po) and installs them where
# the app looks for them: the share/locale next to its bin directory.
#
#     apps/desktop/po/install.sh /app/share/locale
#     apps/desktop/po/install.sh target/share/locale
#
# The second form is for running a build from the source tree in another
# language. Needs msgfmt from GNU gettext.
set -eu
[ $# -eq 1 ] || { echo "usage: $0 <locale directory>" >&2; exit 2; }
here=$(cd "$(dirname "$0")" && pwd)
for po in "$here"/*.po; do
    # No translations at all: the pattern stays as it is.
    [ -e "$po" ] || continue
    language=$(basename "$po" .po)
    mkdir -p "$1/$language/LC_MESSAGES"
    # A catalog with a mistake that would garble a text, such as a placeholder
    # gone missing, isn't installed.
    msgfmt --check -o "$1/$language/LC_MESSAGES/specto.mo" "$po"
    echo "$language: $(msgfmt --statistics -o /dev/null "$po" 2>&1)"
done
