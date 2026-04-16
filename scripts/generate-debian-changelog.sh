#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

usage() {
  cat <<'EOF'
Usage: scripts/generate-debian-changelog.sh [options]

Generate debian/changelog from git commit subjects.

Options:
  --debian-version VERSION  Set Debian package version explicitly
  --distribution DIST       Set changelog distribution (default: unstable)
  --urgency LEVEL           Set urgency (default: medium)
  --since-ref REF           Collect commits after REF
  --until-ref REF           End commit range at REF (default: HEAD)
  --maintainer NAME         Override maintainer name
  --email EMAIL             Override maintainer email
  --output PATH             Write to a custom changelog path
  --append-existing         Keep existing lower changelog entries under new entry
  --include-merges          Include merge commits
  --help                    Show this help

Environment:
  DEBFULLNAME, DEBEMAIL can be used for maintainer identity.
EOF
}

log() {
  printf '[generate-debian-changelog] %s\n' "$*"
}

die() {
  printf '[generate-debian-changelog] %s\n' "$*" >&2
  exit 1
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || die "missing required command: $1"
}

extract_cargo_value() {
  local key="$1"
  sed -nE "s/^${key}[[:space:]]*=[[:space:]]*\"([^\"]+)\"/\1/p" "${REPO_ROOT}/Cargo.toml" | head -n 1
}

default_since_ref() {
  if git -C "${REPO_ROOT}" describe --tags --abbrev=0 >/dev/null 2>&1; then
    git -C "${REPO_ROOT}" describe --tags --abbrev=0
    return
  fi

  git -C "${REPO_ROOT}" rev-list --max-parents=0 HEAD | tail -n 1
}

sanitize_subject() {
  local subject="$1"
  subject="${subject#"${subject%%[![:space:]]*}"}"
  subject="${subject%"${subject##*[![:space:]]}"}"
  subject="${subject%$'\r'}"
  printf '%s' "$subject"
}

main() {
  require_command git
  require_command sed
  require_command date

  local cargo_version package_name
  cargo_version="$(extract_cargo_value "version")"
  package_name="$(extract_cargo_value "name")"

  [[ -n "${cargo_version}" ]] || die "failed to read version from Cargo.toml"
  [[ -n "${package_name}" ]] || die "failed to read package name from Cargo.toml"

  local debian_version="${cargo_version}-1"
  local distribution="unstable"
  local urgency="medium"
  local since_ref=""
  local until_ref="HEAD"
  local maintainer="${DEBFULLNAME:-$(git -C "${REPO_ROOT}" config user.name || true)}"
  local email="${DEBEMAIL:-$(git -C "${REPO_ROOT}" config user.email || true)}"
  local output="${REPO_ROOT}/debian/changelog"
  local append_existing=0
  local include_merges=0

  while [[ $# -gt 0 ]]; do
    case "$1" in
      --debian-version)
        debian_version="${2:?missing value for --debian-version}"
        shift 2
        ;;
      --distribution)
        distribution="${2:?missing value for --distribution}"
        shift 2
        ;;
      --urgency)
        urgency="${2:?missing value for --urgency}"
        shift 2
        ;;
      --since-ref)
        since_ref="${2:?missing value for --since-ref}"
        shift 2
        ;;
      --until-ref)
        until_ref="${2:?missing value for --until-ref}"
        shift 2
        ;;
      --maintainer)
        maintainer="${2:?missing value for --maintainer}"
        shift 2
        ;;
      --email)
        email="${2:?missing value for --email}"
        shift 2
        ;;
      --output)
        output="${2:?missing value for --output}"
        shift 2
        ;;
      --append-existing)
        append_existing=1
        shift
        ;;
      --include-merges)
        include_merges=1
        shift
        ;;
      --help)
        usage
        exit 0
        ;;
      *)
        die "unknown option: $1"
        ;;
    esac
  done

  [[ -n "${maintainer}" ]] || die "maintainer name is empty; set git user.name or DEBFULLNAME"
  [[ -n "${email}" ]] || die "maintainer email is empty; set git user.email or DEBEMAIL"

  if [[ -z "${since_ref}" ]]; then
    since_ref="$(default_since_ref)"
  fi

  git -C "${REPO_ROOT}" rev-parse --verify "${until_ref}^{commit}" >/dev/null 2>&1 || die "invalid until ref: ${until_ref}"
  git -C "${REPO_ROOT}" rev-parse --verify "${since_ref}^{commit}" >/dev/null 2>&1 || die "invalid since ref: ${since_ref}"

  local range="${since_ref}..${until_ref}"
  local log_args=( -C "${REPO_ROOT}" log --format=%s )
  if [[ "${include_merges}" -eq 0 ]]; then
    log_args+=( --no-merges )
  fi
  log_args+=( "${range}" )

  mapfile -t subjects < <(git "${log_args[@]}")

  if [[ "${#subjects[@]}" -eq 0 ]]; then
    log "no commits found in ${range}; using ${until_ref} subject as fallback"
    mapfile -t subjects < <(git -C "${REPO_ROOT}" log -1 --format=%s "${until_ref}")
  fi

  mkdir -p "$(dirname "${output}")"

  local timestamp existing_content=""
  timestamp="$(LC_ALL=C date -R)"
  if [[ "${append_existing}" -eq 1 && -f "${output}" ]]; then
    existing_content="$(cat "${output}")"
  fi

  {
    printf '%s (%s) %s; urgency=%s\n\n' "${package_name}" "${debian_version}" "${distribution}" "${urgency}"

    local rendered=0
    local subject sanitized
    for subject in "${subjects[@]}"; do
      sanitized="$(sanitize_subject "${subject}")"
      [[ -n "${sanitized}" ]] || continue
      printf '  * %s\n' "${sanitized}"
      rendered=1
    done

    if [[ "${rendered}" -eq 0 ]]; then
      printf '  * Update package contents.\n'
    fi

    printf '\n -- %s <%s>  %s\n' "${maintainer}" "${email}" "${timestamp}"

    if [[ -n "${existing_content}" ]]; then
      printf '\n%s\n' "${existing_content}"
    fi
  } > "${output}"

  log "wrote ${output} from git range ${range}"
}

main "$@"
