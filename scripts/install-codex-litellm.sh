#!/bin/bash
# Install the custom Apple Silicon build without replacing the official codex CLI.
set -euo pipefail
umask 077

version=0.166.0
archive_name="codex-litellm-$version-macos-arm64.tar.gz"
archive_sha=2836c4896295939ea4947102a904f4563c693363ce46463d722d85c2de95a664
release_url="https://github.com/zjsng-trav/codex/releases/download/codex-litellm-v$version"
prefix="$HOME/.local/share/codex-litellm/$version"
config_dir="${CODEX_HOME:-$HOME/.codex}"
gateway=
shell_rc=
dry_run=0

die() { printf 'codex-litellm setup: %s\n' "$*" >&2; exit 1; }

while [ "$#" -gt 0 ]; do
    case "$1" in
        --gateway|--prefix|--shell-rc)
            [ "$#" -ge 2 ] || die "missing value for $1"
            case "$1" in
                --gateway) gateway=$2 ;;
                --prefix) prefix=$2 ;;
                --shell-rc) shell_rc=$2 ;;
            esac
            shift 2
            ;;
        --dry-run) dry_run=1; shift ;;
        --help|-h)
            printf '%s\n' 'Usage: bash install-codex-litellm.sh [--gateway https://gateway/v1] [--dry-run]' \
                'Advanced: --prefix /absolute/path --shell-rc /absolute/path' \
                'Apple Silicon macOS + zsh. Prompts for the gateway and your own key when needed.'
            exit 0
            ;;
        *) die "unknown option: $1" ;;
    esac
done

[ "$(uname -s)" = Darwin ] && [ "$(uname -m)" = arm64 ] || die 'this package requires an Apple Silicon Mac'
[ "${SHELL##*/}" = zsh ] || die 'automatic shortcut setup currently supports zsh; see the manual package README for other shells'
for tool in curl shasum tar security; do
    command -v "$tool" >/dev/null || die "required command is missing: $tool"
done

default_rc=$(/bin/zsh -lic 'print -r -- "__CODEX_RC__${ZDOTDIR:-$HOME}/.zshrc"' 2>/dev/null | sed -n 's/^__CODEX_RC__//p')
[ -n "$default_rc" ] || die 'could not locate your zsh configuration'
shell_rc=${shell_rc:-$default_rc}
for path in "$prefix" "$config_dir" "$shell_rc"; do
    [[ "$path" = /* && "$path" != *$'\n'* ]] || die 'installation and configuration paths must be absolute, single-line paths'
done

if [ -z "$gateway" ]; then
    [ -t 0 ] || die 'provide --gateway with your LiteLLM Responses base URL'
    read -r -p 'LiteLLM HTTPS base URL (including /v1): ' gateway
fi
gateway=${gateway%/}
[[ "$gateway" =~ ^https://[A-Za-z0-9._:/%~-]+$ ]] || die 'use an HTTPS gateway URL without credentials, query parameters, or spaces'
catalog_path="$prefix/libexec/codex-litellm/litellm-models.json"
catalog_toml=${catalog_path//\\/\\\\}
catalog_toml=${catalog_toml//\"/\\\"}
profile_text=$(cat <<EOF
# Managed by zjsng-trav/codex LiteLLM setup $version.
model_provider = "litellm"
model = "gpt-6.1-sol"
model_reasoning_effort = "high"
service_tier = "flex"
model_catalog_json = "$catalog_toml"

[model_providers.litellm]
name = "LiteLLM"
base_url = "$gateway"
env_key = "CODEX_LITELLM_API_KEY"
wire_api = "responses"
requires_openai_auth = false

[tui]
status_line = ["model-with-reasoning", "service-tier", "project-name", "context-used", "estimated-thread-cost"]
EOF
)

# Check aliases, functions, and executables in the user's configured login shell.
# Exit codes distinguish an unused name from a shell startup failure.
for attempt in $(seq 0 99); do
    case "$attempt" in
        0) shortcut=codex-litellm ;;
        1) shortcut=codex-litellm-jet ;;
        *) shortcut="codex-litellm-jet-$attempt" ;;
    esac
    launcher="$prefix/launchers/$shortcut"
    profile="$config_dir/$shortcut.config.toml"
    printf -v alias_command '%q' "$launcher"
    if /bin/zsh -lic '
        if [[ "$1" != "$4" && -f "$1" ]]; then source "$1"; fi
        if [[ ${aliases[$2]-} == "$3" ]]; then exit 0; fi
        if whence "$2" >/dev/null 2>&1; then exit 11; fi
        exit 10
    ' codex-litellm-setup "$shell_rc" "$shortcut" "$alias_command" "$default_rc" >/dev/null 2>&1; then
        name_status=0
    else
        name_status=$?
    fi
    if [ "$name_status" = 0 ] && [ -x "$launcher" ] && cmp -s "$profile" <(printf '%s\n' "$profile_text"); then
        break
    fi
    if [ "$name_status" = 10 ] && [ ! -e "$profile" ] && [ ! -L "$profile" ] && [ ! -e "$launcher" ] && [ ! -L "$launcher" ]; then
        break
    fi
    [ "$name_status" = 0 ] || [ "$name_status" = 10 ] || [ "$name_status" = 11 ] || die 'could not safely inspect existing shell shortcuts'
    [ "$attempt" != 99 ] || die 'no unused shortcut name found'
done
printf -v alias_line 'alias %s=%q' "$shortcut" "$alias_command"
printf '%s\n' "Shortcut: $shortcut" "Profile: $profile" "Install: $prefix" "Shell config: $shell_rc"
if [ "$dry_run" = 1 ]; then
    printf '%s\n' 'Dry run complete. No download, profile, key, or shortcut was written.'
    exit 0
fi

if [ ! -x "$prefix/libexec/codex-litellm/codex" ]; then
    download_dir=$(mktemp -d "${TMPDIR:-/tmp}/codex-litellm-setup.XXXXXX")
    trap 'rm -rf "$download_dir"' EXIT
    curl --proto '=https' --tlsv1.2 -fL "$release_url/$archive_name" -o "$download_dir/$archive_name"
    printf '%s  %s\n' "$archive_sha" "$download_dir/$archive_name" | shasum -a 256 -c -
    tar -xzf "$download_dir/$archive_name" -C "$download_dir"
    sh "$download_dir/codex-litellm-$version-macos-arm64/install.sh" --prefix "$prefix"
fi
printf '%s  %s\n' \
    f8df23f6d1ea3a27702ae5caa378711ba026c34b771f023cf115a24e2891411b "$prefix/libexec/codex-litellm/codex" \
    3a38dc87c867b7c0430388f3097ded15453c42848eb87a2a1048f98a40024efb "$prefix/libexec/codex-litellm/codex-code-mode-host" | shasum -a 256 -c -
[ -f "$catalog_path" ] || die 'the packaged model catalog is missing'

if [ -z "${CODEX_LITELLM_API_KEY:-}" ] && ! security find-generic-password -a "$USER" -s codex-litellm -w >/dev/null 2>&1; then
    [ -t 0 ] || die 'run interactively to save your key, or supply CODEX_LITELLM_API_KEY in the environment'
    read -r -s -p 'Paste your own LiteLLM API key (hidden): ' litellm_key
    printf '\n'
    [ -n "$litellm_key" ] || die 'the key must not be empty'
    security add-generic-password -U -a "$USER" -s codex-litellm -w "$litellm_key" >/dev/null
    unset litellm_key
fi

mkdir -p "$config_dir" "$prefix/launchers" "$(dirname "$shell_rc")"
if [ ! -e "$profile" ] && [ ! -L "$profile" ]; then
    (set -o noclobber; printf '%s\n' "$profile_text" > "$profile")
fi
if [ ! -e "$launcher" ] && [ ! -L "$launcher" ]; then
    (set -o noclobber; cat > "$launcher" <<EOF
#!/bin/sh
# Managed by zjsng-trav/codex LiteLLM setup $version.
set -eu
launcher_dir=\$(CDPATH= cd -- "\$(dirname -- "\$0")" && pwd)
binary="\$launcher_dir/../libexec/codex-litellm/codex"
case "\${1:-}" in --version|-V|--help|-h) exec "\$binary" "\$@" ;; esac
if [ -z "\${CODEX_LITELLM_API_KEY:-}" ]; then
    CODEX_LITELLM_API_KEY=\$(security find-generic-password -a "\$USER" -s codex-litellm -w 2>/dev/null) || {
        printf '%s\\n' 'No LiteLLM key found. Rerun setup or set CODEX_LITELLM_API_KEY.' >&2
        exit 1
    }
    export CODEX_LITELLM_API_KEY
fi
exec "\$binary" --no-daemon -p "$shortcut" "\$@"
EOF
    )
    chmod 0755 "$launcher"
fi
if [ ! -f "$shell_rc" ] || ! grep -Fqx "$alias_line" "$shell_rc"; then
    if [ -f "$shell_rc" ]; then cp -p "$shell_rc" "$shell_rc.codex-litellm-backup.$(date +%Y%m%d%H%M%S).$$"; fi
    printf '\n# Codex LiteLLM %s — separate from the official codex command\n%s\n' "$version" "$alias_line" >> "$shell_rc"
fi
"$launcher" --version
printf '\nReady. Open a new terminal, then run: %s\n' "$shortcut"
printf 'This profile starts in Slow/Flex mode; /slow toggles it. Your official codex command is unchanged.\n'
