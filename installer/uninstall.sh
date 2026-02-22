#!/usr/bin/env bash
set -euo pipefail

PROJECT="claw-screenshot"
USER_ONLY=0

usage() {
  cat <<EOF
Usage: uninstall.sh [--user]

Uninstalls ${PROJECT}.
  --user   Remove only user-local files (~/.local/*) and skip package manager removal.
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --user)
      USER_ONLY=1
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage
      exit 2
      ;;
  esac
  shift
done

run_privileged() {
  if [ "$(id -u)" -eq 0 ]; then
    "$@"
  elif command -v sudo >/dev/null 2>&1; then
    sudo "$@"
  else
    echo "sudo not available; cannot run: $*" >&2
    return 1
  fi
}

remove_package_install() {
  local removed=0

  if command -v dpkg-query >/dev/null 2>&1 &&
     dpkg-query -W -f='${Status}' "${PROJECT}" 2>/dev/null | grep -q "ok installed"; then
    if command -v apt-get >/dev/null 2>&1; then
      run_privileged apt-get remove -y "${PROJECT}"
    else
      run_privileged dpkg -r "${PROJECT}"
    fi
    removed=1
  fi

  if [ "${removed}" -eq 0 ] &&
     command -v rpm >/dev/null 2>&1 &&
     rpm -q "${PROJECT}" >/dev/null 2>&1; then
    if command -v dnf >/dev/null 2>&1; then
      run_privileged dnf remove -y "${PROJECT}"
    elif command -v yum >/dev/null 2>&1; then
      run_privileged yum remove -y "${PROJECT}"
    else
      run_privileged rpm -e "${PROJECT}"
    fi
    removed=1
  fi

  [ "${removed}" -eq 1 ]
}

remove_user_install() {
  local removed=0
  local bin_path="${HOME}/.local/bin/${PROJECT}"
  local desktop_file="${HOME}/.local/share/applications/${PROJECT}.desktop"
  local icon_root="${HOME}/.local/share/icons/hicolor"
  local size

  if [ -f "${bin_path}" ]; then
    rm -f "${bin_path}"
    removed=1
  fi

  if [ -f "${desktop_file}" ]; then
    rm -f "${desktop_file}"
    removed=1
  fi

  for size in 16 32 48 64 128 256 512; do
    local icon="${icon_root}/${size}x${size}/apps/${PROJECT}.png"
    if [ -f "${icon}" ]; then
      rm -f "${icon}"
      removed=1
    fi
  done

  if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "${HOME}/.local/share/applications" || true
  fi
  if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -q -t -f "${icon_root}" || true
  fi

  [ "${removed}" -eq 1 ]
}

pkg_removed=0
if [ "${USER_ONLY}" -eq 0 ]; then
  if remove_package_install; then
    pkg_removed=1
  fi
fi

user_removed=0
if remove_user_install; then
  user_removed=1
fi

if [ "${pkg_removed}" -eq 0 ] && [ "${user_removed}" -eq 0 ]; then
  echo "Nothing to uninstall for ${PROJECT}."
else
  echo "Uninstall complete for ${PROJECT}."
fi
