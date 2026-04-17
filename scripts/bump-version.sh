#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
CARGO_TOML="${REPO_ROOT}/Cargo.toml"
CARGO_LOCK="${REPO_ROOT}/Cargo.lock"
DEBIAN_CHANGELOG="${REPO_ROOT}/debian/changelog"

usage() {
  cat <<'EOF'
Usage: scripts/bump-version.sh [options] [VERSION]

Bump project version in Cargo.toml and Debian packaging version in debian/changelog.

Options:
  --version VERSION         Set project version explicitly
  --bump TYPE               Bump project version by TYPE (major|minor|patch)
  --debian-version VERSION  Set Debian package version explicitly
  --debian-revision N       Debian revision suffix when --debian-version is omitted (default: 1)
  --dry-run                 Show computed changes without modifying files
  --help                    Show this help

Examples:
  scripts/bump-version.sh --version 0.2.0
  scripts/bump-version.sh --bump patch
  scripts/bump-version.sh --version 1.0.0 --debian-revision 2
  scripts/bump-version.sh --version 1.0.0 --debian-version 1.0.0-3
EOF
}

log() {
  printf '[bump-version] %s\n' "$*"
}

die() {
  printf '[bump-version] %s\n' "$*" >&2
  exit 1
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || die "missing required command: $1"
}

read_cargo_package_version() {
  local file="$1"

  awk '
    BEGIN { in_package = 0; found = 0 }

    /^\[package\][[:space:]]*$/ {
      in_package = 1
      next
    }

    /^\[[^]]+\][[:space:]]*$/ {
      in_package = 0
    }

    in_package && match($0, /^[[:space:]]*version[[:space:]]*=[[:space:]]*"([^"]+)"/, m) {
      print m[1]
      found = 1
      exit 0
    }

    END {
      if (!found) {
        exit 1
      }
    }
  ' "${file}"
}

read_cargo_package_name() {
  local file="$1"

  awk '
    BEGIN { in_package = 0; found = 0 }

    /^\[package\][[:space:]]*$/ {
      in_package = 1
      next
    }

    /^\[[^]]+\][[:space:]]*$/ {
      in_package = 0
    }

    in_package && match($0, /^[[:space:]]*name[[:space:]]*=[[:space:]]*"([^"]+)"/, m) {
      print m[1]
      found = 1
      exit 0
    }

    END {
      if (!found) {
        exit 1
      }
    }
  ' "${file}"
}

read_cargo_lock_package_version() {
  local file="$1"
  local package_name="$2"

  awk -v package_name="${package_name}" '
    BEGIN {
      in_package = 0
      target_package = 0
      found = 0
    }

    /^\[\[package\]\][[:space:]]*$/ {
      in_package = 1
      target_package = 0
      next
    }

    {
      if (in_package && match($0, /^[[:space:]]*name[[:space:]]*=[[:space:]]*"([^"]+)"[[:space:]]*$/, m)) {
        target_package = (m[1] == package_name)
      }

      if (in_package && target_package && match($0, /^[[:space:]]*version[[:space:]]*=[[:space:]]*"([^"]+)"[[:space:]]*$/, m)) {
        print m[1]
        found = 1
        exit 0
      }
    }

    END {
      if (!found) {
        exit 1
      }
    }
  ' "${file}"
}

read_debian_changelog_version() {
  local file="$1"

  sed -nE '1s/^[^[:space:]]+[[:space:]]+\(([^)]+)\).*/\1/p' "${file}" | head -n 1
}

validate_project_version() {
  local version="$1"

  [[ "${version}" =~ ^[0-9]+\.[0-9]+\.[0-9]+([-.+][0-9A-Za-z.-]+)?$ ]] || \
    die "invalid project version '${version}' (expected semantic version like 1.2.3)"
}

compute_bumped_version() {
  local current="$1"
  local bump_type="$2"

  if [[ ! "${current}" =~ ^([0-9]+)\.([0-9]+)\.([0-9]+)$ ]]; then
    die "cannot use --bump with non-numeric version '${current}'; use --version instead"
  fi

  local major="${BASH_REMATCH[1]}"
  local minor="${BASH_REMATCH[2]}"
  local patch="${BASH_REMATCH[3]}"

  case "${bump_type}" in
    major)
      major=$((major + 1))
      minor=0
      patch=0
      ;;
    minor)
      minor=$((minor + 1))
      patch=0
      ;;
    patch)
      patch=$((patch + 1))
      ;;
    *)
      die "invalid bump type '${bump_type}' (expected major, minor, or patch)"
      ;;
  esac

  printf '%s.%s.%s\n' "${major}" "${minor}" "${patch}"
}

render_updated_cargo_toml() {
  local file="$1"
  local new_version="$2"

  awk -v new_version="${new_version}" '
    BEGIN { in_package = 0; updated = 0 }

    /^\[package\][[:space:]]*$/ {
      in_package = 1
      print
      next
    }

    /^\[[^]]+\][[:space:]]*$/ {
      in_package = 0
      print
      next
    }

    {
      if (in_package && !updated && match($0, /^([[:space:]]*)version[[:space:]]*=[[:space:]]*"[^"]*"[[:space:]]*$/, m)) {
        print m[1] "version = \"" new_version "\""
        updated = 1
        next
      }

      print
    }

    END {
      if (!updated) {
        print "failed to find [package] version in Cargo.toml" > "/dev/stderr"
        exit 1
      }
    }
  ' "${file}"
}

render_updated_debian_changelog() {
  local file="$1"
  local new_debian_version="$2"

  awk -v new_debian_version="${new_debian_version}" '
    NR == 1 {
      if (match($0, /^([^[:space:]]+[[:space:]]+\()[^)]+(\).*)$/, m)) {
        print m[1] new_debian_version m[2]
        next
      }

      print "failed to parse first line in debian/changelog" > "/dev/stderr"
      exit 1
    }

    { print }
  ' "${file}"
}

render_updated_cargo_lock() {
  local file="$1"
  local package_name="$2"
  local new_version="$3"

  awk -v package_name="${package_name}" -v new_version="${new_version}" '
    BEGIN {
      in_package = 0
      target_package = 0
      updated = 0
    }

    /^\[\[package\]\][[:space:]]*$/ {
      in_package = 1
      target_package = 0
      print
      next
    }

    {
      if (in_package && match($0, /^[[:space:]]*name[[:space:]]*=[[:space:]]*"([^"]+)"[[:space:]]*$/, m)) {
        target_package = (!updated && m[1] == package_name)
        print
        next
      }

      if (in_package && target_package && match($0, /^([[:space:]]*)version[[:space:]]*=[[:space:]]*"[^"]*"[[:space:]]*$/, m)) {
        print m[1] "version = \"" new_version "\""
        updated = 1
        target_package = 0
        next
      }

      print
    }

    END {
      if (!updated) {
        print "failed to find target package version in Cargo.lock" > "/dev/stderr"
        exit 1
      }
    }
  ' "${file}"
}

main() {
  require_command awk
  require_command sed
  require_command mktemp

  [[ -f "${CARGO_TOML}" ]] || die "missing file: ${CARGO_TOML}"
  [[ -f "${DEBIAN_CHANGELOG}" ]] || die "missing file: ${DEBIAN_CHANGELOG}"

  local explicit_version=""
  local bump_type=""
  local debian_version=""
  local debian_revision="1"
  local debian_revision_set=0
  local dry_run=0

  while [[ $# -gt 0 ]]; do
    case "$1" in
      --version)
        explicit_version="${2:?missing value for --version}"
        shift 2
        ;;
      --bump)
        bump_type="${2:?missing value for --bump}"
        shift 2
        ;;
      --debian-version)
        debian_version="${2:?missing value for --debian-version}"
        shift 2
        ;;
      --debian-revision)
        debian_revision="${2:?missing value for --debian-revision}"
        debian_revision_set=1
        shift 2
        ;;
      --dry-run)
        dry_run=1
        shift
        ;;
      --help)
        usage
        exit 0
        ;;
      --*)
        die "unknown option: $1"
        ;;
      *)
        if [[ -z "${explicit_version}" && -z "${bump_type}" ]]; then
          explicit_version="$1"
          shift
        else
          die "unexpected positional argument: $1"
        fi
        ;;
    esac
  done

  if [[ -n "${explicit_version}" && -n "${bump_type}" ]]; then
    die "use either --version or --bump, not both"
  fi

  if [[ -z "${explicit_version}" && -z "${bump_type}" ]]; then
    die "missing required version input; use --version, --bump, or positional VERSION"
  fi

  if [[ -n "${debian_version}" && "${debian_revision_set}" -eq 1 ]]; then
    die "--debian-version cannot be combined with --debian-revision"
  fi

  if [[ ! "${debian_revision}" =~ ^[0-9]+$ ]] || [[ "${debian_revision}" -lt 1 ]]; then
    die "invalid --debian-revision '${debian_revision}' (expected integer >= 1)"
  fi

  local current_project_version
  current_project_version="$(read_cargo_package_version "${CARGO_TOML}")" || die "failed to read current Cargo.toml version"

  local package_name
  package_name="$(read_cargo_package_name "${CARGO_TOML}")" || die "failed to read package name from Cargo.toml"

  local has_cargo_lock=0
  local current_lock_version=""
  if [[ -f "${CARGO_LOCK}" ]]; then
    current_lock_version="$(read_cargo_lock_package_version "${CARGO_LOCK}" "${package_name}")" || \
      die "failed to read root package version from Cargo.lock"
    has_cargo_lock=1
  fi

  local current_debian_version
  current_debian_version="$(read_debian_changelog_version "${DEBIAN_CHANGELOG}")"
  [[ -n "${current_debian_version}" ]] || die "failed to read current Debian changelog version"

  local next_project_version=""
  if [[ -n "${explicit_version}" ]]; then
    next_project_version="${explicit_version}"
  else
    next_project_version="$(compute_bumped_version "${current_project_version}" "${bump_type}")"
  fi

  validate_project_version "${next_project_version}"

  local next_debian_version="${debian_version}"
  if [[ -z "${next_debian_version}" ]]; then
    next_debian_version="${next_project_version}-${debian_revision}"
  fi

  if [[ "${dry_run}" -eq 1 ]]; then
    log "dry-run only; no files will be modified"
    log "Cargo.toml version: ${current_project_version} -> ${next_project_version}"
    if [[ "${has_cargo_lock}" -eq 1 ]]; then
      log "Cargo.lock package version (${package_name}): ${current_lock_version} -> ${next_project_version}"
    fi
    log "debian/changelog version: ${current_debian_version} -> ${next_debian_version}"
    exit 0
  fi

  local tmp_cargo=""
  local tmp_changelog=""
  local tmp_lock=""
  tmp_cargo="$(mktemp)"
  tmp_changelog="$(mktemp)"
  if [[ "${has_cargo_lock}" -eq 1 ]]; then
    tmp_lock="$(mktemp)"
  fi
  trap 'rm -f "${tmp_cargo:-}" "${tmp_changelog:-}" "${tmp_lock:-}"' EXIT

  render_updated_cargo_toml "${CARGO_TOML}" "${next_project_version}" > "${tmp_cargo}" || \
    die "failed to render updated Cargo.toml"
  render_updated_debian_changelog "${DEBIAN_CHANGELOG}" "${next_debian_version}" > "${tmp_changelog}" || \
    die "failed to render updated debian/changelog"
  if [[ "${has_cargo_lock}" -eq 1 ]]; then
    render_updated_cargo_lock "${CARGO_LOCK}" "${package_name}" "${next_project_version}" > "${tmp_lock}" || \
      die "failed to render updated Cargo.lock"
  fi

  cat "${tmp_cargo}" > "${CARGO_TOML}"
  cat "${tmp_changelog}" > "${DEBIAN_CHANGELOG}"
  if [[ "${has_cargo_lock}" -eq 1 ]]; then
    cat "${tmp_lock}" > "${CARGO_LOCK}"
  fi

  log "updated Cargo.toml version: ${current_project_version} -> ${next_project_version}"
  if [[ "${has_cargo_lock}" -eq 1 ]]; then
    log "updated Cargo.lock package version (${package_name}): ${current_lock_version} -> ${next_project_version}"
  fi
  log "updated debian/changelog version: ${current_debian_version} -> ${next_debian_version}"
}

main "$@"
