#!/usr/bin/env bash
# System packages needed to build and test Specto on Fedora. Shared by GitHub
# Actions and (later) GitLab CI so both run the same environment.
#
#   ci/install-deps-fedora.sh             building, clippy and cargo test
#   ci/install-deps-fedora.sh --headless  also the desktop app's headless checks
set -euo pipefail
packages=(
  gcc pkgconf-pkg-config git curl
  gtk4-devel libadwaita-devel
  # Reference tools used by tests: qpdf checks the files specto-write saves.
  poppler-utils qpdf
)
if [[ "${1:-}" == "--headless" ]]; then
  # apps/desktop/test-ui: a compositor, a session bus, gdbus, Python with GObject
  # (the check script), and software OpenGL. For the check of the translations,
  # msgfmt and a locale other than C, which has none.
  packages+=(mutter dbus-daemon glib2 python3 python3-gobject mesa-dri-drivers)
  packages+=(gettext glibc-langpack-en)
fi
dnf install -y --setopt=install_weak_deps=False "${packages[@]}"
