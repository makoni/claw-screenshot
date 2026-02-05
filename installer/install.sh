#!/usr/bin/env bash
set -euo pipefail

OWNER="makoni"
REPO="claw-screenshot"
PROJECT="claw-screenshot"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

usage() {
  cat <<EOF
Usage: install.sh [--version <version>] [--user]

Installs ${PROJECT} from GitHub Releases.
  --version <version>  Install a specific version (e.g. 0.1.0). Defaults to latest.
  --user               Install to ~/.local/bin (skip .deb/.rpm even if available).
EOF
}

VERSION="latest"
USER_ONLY=0

while [ "$#" -gt 0 ]; do
  case "$1" in
    --version)
      shift
      [ "$#" -gt 0 ] || { echo "Missing value for --version" >&2; exit 2; }
      VERSION="$1"
      ;;
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

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || { echo "Missing required command: $1" >&2; exit 2; }
}

need_cmd curl
need_cmd sha256sum
need_cmd tar
need_cmd awk
need_cmd uname
need_cmd id

PYTHON=""
if command -v python3 >/dev/null 2>&1; then
  PYTHON="python3"
elif command -v python >/dev/null 2>&1; then
  PYTHON="python"
fi

TAG=""
VERSION_NUM=""
MANIFEST_AVAILABLE=0

fetch_manifest() {
  local tag="$1"
  local url="https://github.com/${OWNER}/${REPO}/releases/download/${tag}/artifacts-manifest.json"
  echo "Fetching manifest: ${url}"
  if curl -fsSL -o "${TMPDIR}/manifest.json" "${url}"; then
    TAG="${tag}"
    MANIFEST_AVAILABLE=1
    return 0
  fi
  return 1
}

TMPDIR="$(mktemp -d)"
trap 'rm -rf "$TMPDIR"' EXIT

if [ "${VERSION}" = "latest" ]; then
  TAG="$(curl -fsSL "https://api.github.com/repos/${OWNER}/${REPO}/releases/latest" | awk -F '"' '/"tag_name":/ {print $4; exit}')"
  [ -n "${TAG}" ] || { echo "Unable to determine latest tag" >&2; exit 3; }
  fetch_manifest "${TAG}" || echo "Manifest not found; will use tarball checksums if available."
else
  TAG="${VERSION}"
  if ! fetch_manifest "${TAG}"; then
    if [[ "${TAG}" == v* ]]; then
      ALT="${TAG#v}"
    else
      ALT="v${TAG}"
    fi
    fetch_manifest "${ALT}" || echo "Manifest not found; will use tarball checksums if available."
  fi
fi

VERSION_NUM="${TAG#v}"

detect_arch() {
  case "$(uname -m)" in
    x86_64) echo "amd64" ;;
    aarch64|arm64) echo "arm64" ;;
    *) echo "unsupported" ;;
  esac
}

ARCH="$(detect_arch)"
if [ "${ARCH}" = "unsupported" ]; then
  echo "Unsupported architecture: $(uname -m)" >&2
  exit 4
fi

OS="$(uname -s)"
if [ "${OS}" != "Linux" ]; then
  echo "Unsupported OS: ${OS}" >&2
  exit 4
fi

pick_asset() {
  local suffix="$1"
  local file
  file="$(awk -F '"' -v s="${suffix}" '/"file"/ {if ($4 ~ s "$") {print $4; exit}}' "${TMPDIR}/manifest.json")"
  if [ -n "${file}" ]; then
    echo "${file}"
  fi
}

sha_for_asset() {
  local file="$1"
  awk -F '"' -v f="${file}" '{
    for (i=1; i<=NF; i++) {
      if ($i=="file" && $(i+2)==f) {
        for (j=i+3; j<=NF; j++) {
          if ($j=="sha256") {print $(j+2); exit}
        }
      }
    }
  }' "${TMPDIR}/manifest.json"
}

fetch_release_assets() {
  [ -n "${PYTHON}" ] || return 1
  [ -f "${TMPDIR}/release.json" ] && return 0
  curl -fsSL -o "${TMPDIR}/release.json" "https://api.github.com/repos/${OWNER}/${REPO}/releases/tags/${TAG}" || return 1
}

pick_asset_from_release() {
  local pattern="$1"
  fetch_release_assets || return 1
  "${PYTHON}" - <<'PY' "${pattern}" "${TMPDIR}/release.json"
import json
import re
import sys
from pathlib import Path

pattern = re.compile(sys.argv[1])
data = json.loads(Path(sys.argv[2]).read_text())
for asset in data.get("assets", []):
    name = asset.get("name", "")
    if pattern.search(name):
        print(name)
        break
PY
}

url_exists() {
  curl -fsI "$1" >/dev/null 2>&1
}

download_and_verify() {
  local url="$1"
  local sha_expected="$2"
  local out="$3"
  echo "Downloading ${url}..."
  if ! curl -fL --retry 3 --retry-delay 1 -o "${out}" "${url}"; then
    return 1
  fi
  local sha_actual
  sha_actual="$(sha256sum "${out}" | awk '{print $1}')"
  if [ "${sha_actual}" != "${sha_expected}" ]; then
    echo "Checksum mismatch for ${url}" >&2
    return 2
  fi
}

install_desktop_entry() {
  local bin_path="$1"
  local desktop_dir="${HOME}/.local/share/applications"
  mkdir -p "${desktop_dir}"
  cat > "${desktop_dir}/${PROJECT}.desktop" <<DESK
[Desktop Entry]
Type=Application
Name=Claw Screenshot Helper
Exec=${bin_path}
Icon=claw-screenshot
Terminal=false
Categories=Utility;
StartupNotify=true
DESK
  if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "${desktop_dir}" || true
  fi
}

try_package_install() {
  local pkg="$1"
  local sha="$2"
  local url="https://github.com/${OWNER}/${REPO}/releases/download/${TAG}/${pkg}"
  local out="${TMPDIR}/${pkg}"
  if [ -z "${sha}" ]; then
    echo "Missing checksum for ${pkg}; skipping package install." >&2
    return 1
  fi
  local sudo_cmd=""
  if [ "$(id -u)" -ne 0 ]; then
    if command -v sudo >/dev/null 2>&1; then
      sudo_cmd="sudo"
    else
      echo "sudo not available; skipping package install." >&2
      return 1
    fi
  fi
  if ! download_and_verify "${url}" "${sha}" "${out}"; then
    return 1
  fi
  if [[ "${pkg}" == *.deb ]]; then
    if command -v apt-get >/dev/null 2>&1; then
      ${sudo_cmd} dpkg -i "${out}" || ${sudo_cmd} apt-get -f install -y
      return 0
    fi
  fi
  if [[ "${pkg}" == *.rpm ]]; then
    if command -v dnf >/dev/null 2>&1; then
      ${sudo_cmd} dnf install -y "${out}"
      return 0
    elif command -v yum >/dev/null 2>&1; then
      ${sudo_cmd} yum install -y "${out}"
      return 0
    fi
  fi
  return 1
}

install_tarball() {
  local tarball="$1"
  local sha="$2"
  local url="https://github.com/${OWNER}/${REPO}/releases/download/${TAG}/${tarball}"
  local out="${TMPDIR}/${tarball}"
  if [ -z "${sha}" ]; then
    echo "Missing checksum for ${tarball}" >&2
    exit 6
  fi
  if ! download_and_verify "${url}" "${sha}" "${out}"; then
    echo "Failed to download ${tarball}" >&2
    exit 6
  fi
  tar -xzf "${out}" -C "${TMPDIR}"
  local bin_path
  bin_path="$(find "${TMPDIR}" -type f -name "${PROJECT}" -perm /111 | head -n1 || true)"
  if [ -z "${bin_path}" ]; then
    echo "${PROJECT} binary not found in archive" >&2
    exit 6
  fi
  mkdir -p "${HOME}/.local/bin"
  install -m 0755 "${bin_path}" "${HOME}/.local/bin/${PROJECT}"
  install_desktop_entry "${HOME}/.local/bin/${PROJECT}"
  if [ -d "${SCRIPT_DIR}/icons/hicolor" ]; then
    mkdir -p "${HOME}/.local/share/icons/hicolor"
    cp -a "${SCRIPT_DIR}/icons/hicolor/"* "${HOME}/.local/share/icons/hicolor/"
  fi
  echo "Installed to ${HOME}/.local/bin/${PROJECT}"
}

try_package_for_arch() {
  local kind="$1"
  if [ "${MANIFEST_AVAILABLE}" -eq 0 ]; then
    return 1
  fi
  local pkg
  pkg="$(pick_asset "${ARCH}.*\\.${kind}$")"
  if [ -z "${pkg}" ]; then
    return 1
  fi
  local sha
  sha="$(sha_for_asset "${pkg}")"
  if [ -z "${sha}" ]; then
    return 1
  fi
  if try_package_install "${pkg}" "${sha}"; then
    echo "Installed ${pkg} via package manager."
    return 0
  fi
  return 1
}

if [ "${USER_ONLY}" -eq 0 ]; then
  if try_package_for_arch "deb"; then
    exit 0
  fi
  if try_package_for_arch "rpm"; then
    exit 0
  fi
fi

find_tarball_asset() {
  local found=""
  if [ "${MANIFEST_AVAILABLE}" -eq 1 ]; then
    found="$(pick_asset "${ARCH}.*\\.tar\\.gz")"
  fi
  if [ -z "${found}" ]; then
    found="$(pick_asset_from_release "${ARCH}.*\\.tar\\.gz")"
  fi
  if [ -n "${found}" ]; then
    echo "${found}"
  fi
}

TARBALL="$(find_tarball_asset)"
if [ -z "${TARBALL}" ]; then
  TARBALL="${PROJECT}-${ARCH}-${VERSION_NUM}.tar.gz"
fi
if ! url_exists "https://github.com/${OWNER}/${REPO}/releases/download/${TAG}/${TARBALL}"; then
  if [ "${VERSION}" = "latest" ] && [ "${MANIFEST_AVAILABLE}" -eq 1 ]; then
    TARBALL="$(find_tarball_asset)"
  fi
fi
if [ -z "${TARBALL}" ] || ! url_exists "https://github.com/${OWNER}/${REPO}/releases/download/${TAG}/${TARBALL}"; then
  echo "No tarball found for architecture ${ARCH} in ${TAG}" >&2
  exit 7
fi
TARBALL_SHA_URL="https://github.com/${OWNER}/${REPO}/releases/download/${TAG}/${TARBALL}.sha256"
TARBALL_SHA="$(curl -fsSL "${TARBALL_SHA_URL}" | awk '{print $1}')"
if [ -z "${TARBALL_SHA}" ] && [ "${MANIFEST_AVAILABLE}" -eq 1 ]; then
  TARBALL_SHA="$(sha_for_asset "${TARBALL}")"
fi
install_tarball "${TARBALL}" "${TARBALL_SHA}"
echo "Done."
