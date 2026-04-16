#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

usage() {
  cat <<'EOF'
Usage: scripts/build-deb.sh [options] [-- dpkg-buildpackage args...]

Build Debian packages for this project.

Options:
  --generate-changelog       Regenerate debian/changelog before building
  --append-existing          Keep existing lower changelog entries when regenerating
  --since-ref REF            Pass --since-ref to changelog generator
  --until-ref REF            Pass --until-ref to changelog generator
  --debian-version VERSION   Pass --debian-version to changelog generator
  --distribution DIST        Pass --distribution to changelog generator
  --urgency LEVEL            Pass --urgency to changelog generator
  --skip-tests               Disable tests by exporting DEB_BUILD_OPTIONS=nocheck
  --clean                    Remove previous top-level .deb artifacts before building
  --help                     Show this help

Defaults:
  dpkg-buildpackage -us -uc -b
EOF
}

log() {
  printf '[build-deb] %s\n' "$*"
}

die() {
  printf '[build-deb] %s\n' "$*" >&2
  exit 1
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || die "missing required command: $1"
}

main() {
  require_command dpkg-buildpackage

  local generate_changelog=0
  local append_existing=0
  local clean_outputs=0
  local skip_tests=0
  local -a changelog_args=()
  local -a build_args=( -us -uc -b )

  while [[ $# -gt 0 ]]; do
    case "$1" in
      --generate-changelog)
        generate_changelog=1
        shift
        ;;
      --append-existing)
        append_existing=1
        shift
        ;;
      --since-ref|--until-ref|--debian-version|--distribution|--urgency)
        changelog_args+=( "$1" "${2:?missing value for $1}" )
        shift 2
        ;;
      --skip-tests)
        skip_tests=1
        shift
        ;;
      --clean)
        clean_outputs=1
        shift
        ;;
      --help)
        usage
        exit 0
        ;;
      --)
        shift
        build_args+=( "$@" )
        break
        ;;
      *)
        build_args+=( "$1" )
        shift
        ;;
    esac
  done

  if [[ "${generate_changelog}" -eq 1 ]]; then
    local -a generator_cmd=( "${SCRIPT_DIR}/generate-debian-changelog.sh" )
    generator_cmd+=( "${changelog_args[@]}" )
    if [[ "${append_existing}" -eq 1 ]]; then
      generator_cmd+=( --append-existing )
    fi
    log "regenerating debian/changelog"
    ( cd "${REPO_ROOT}" && "${generator_cmd[@]}" )
  fi

  if [[ "${clean_outputs}" -eq 1 ]]; then
    log "removing previous Debian package artifacts from parent directory"
    shopt -s nullglob
    local -a artifacts=(
      "${REPO_ROOT}/../"*.deb
      "${REPO_ROOT}/../"*.buildinfo
      "${REPO_ROOT}/../"*.changes
      "${REPO_ROOT}/../"*.build
    )
    shopt -u nullglob
    if [[ "${#artifacts[@]}" -gt 0 ]]; then
      rm -f "${artifacts[@]}"
    fi
  fi

  if [[ "${skip_tests}" -eq 1 ]]; then
    export DEB_BUILD_OPTIONS="${DEB_BUILD_OPTIONS:-} nocheck"
    log "tests disabled via DEB_BUILD_OPTIONS=${DEB_BUILD_OPTIONS}"
  fi

  log "running: dpkg-buildpackage ${build_args[*]}"
  (
    cd "${REPO_ROOT}"
    dpkg-buildpackage "${build_args[@]}"
  )
}

main "$@"
