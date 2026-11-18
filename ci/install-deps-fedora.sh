#!/usr/bin/env bash
# System packages needed to build and test Specto on Fedora, for CI and for
# running the same checks locally.
#
#   ci/install-deps-fedora.sh             building, clippy and cargo test
#   ci/install-deps-fedora.sh --headless  also the desktop app's headless checks
set -euo pipefail
packages=(
  gcc pkgconf-pkg-config git curl
  gtk4-devel libadwaita-devel
  # qpdf checks the files specto-write saves.
  qpdf
  # xgettext and diff, for the check of the desktop app's translation template.
  gettext diffutils
)
if [[ "${1:-}" == "--headless" ]]; then
  # apps/desktop/test-ui: a compositor, a session bus, gdbus, Python with GObject
  # (the check script), and software OpenGL. For the check of the translations,
  # a locale other than C, which has none. Poppler reads the files the app saves
  # and prints, independently of Specto.
  packages+=(mutter dbus-daemon glib2 python3 python3-gobject mesa-dri-drivers)
  packages+=(glibc-langpack-en poppler-utils)
fi
dnf install -y --setopt=install_weak_deps=False "${packages[@]}"
