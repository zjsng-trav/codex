# Codex LiteLLM

**A lower-cost lane for coding work that can wait.**

A custom [OpenAI Codex](https://github.com/openai/codex) build maintained by [Jet Sng](https://github.com/zjsng-trav), with LiteLLM Flex retries, visible service-tier status, and provider-reported thread costs. Choose **Slow/Flex** for suitable work when your gateway offers lower Flex pricing.

[Download for Apple Silicon](https://github.com/zjsng-trav/codex/releases/tag/codex-litellm-v0.166.0) · [Setup script](scripts/install-codex-litellm.sh) · [Report an issue](https://github.com/zjsng-trav/codex/issues)

> This is a custom fork based on **Codex v0.166.0**, not an official OpenAI distribution. The default `main` contains the custom version. Release binaries were built from [`a5dc3f3aea94`](https://github.com/zjsng-trav/codex/commit/a5dc3f3aea9408df716a2b6d305006e8370273bc); later setup/documentation commits do not require rebuilding them.

## What's different from official Codex?

Official Codex already supports custom model providers and service tiers. This fork adds the following behavior relative to the upstream **`rust-v0.166.0`** baseline:

| Area | Official v0.166.0 baseline | This fork |
| --- | --- | --- |
| LiteLLM Flex capacity errors | Uses the ordinary response-stream retry policy. | Recognized Flex-capacity errors for sampling requests using provider ID `litellm` retry independently of the ordinary stream retry budget, without a fixed attempt limit. |
| Retry timing and tier | Uses the ordinary retry/fallback path. | Local backoff is **5 → 10 → 20 → 40 → 60 seconds**, then 60 seconds. Server retry advice takes precedence. Input and requested tier are retained; capacity retries do not automatically switch to Standard. **Esc** cancels the foreground turn. |
| Thread costs through LiteLLM | Account usage depends on Codex backend authentication and supported workspace plans. | Records **LiteLLM-reported `usage.cost` in USD**, exposes it through the thread-usage API, and displays it with `estimated-thread-cost` without enterprise authentication. |
| Durable cost history | No corresponding LiteLLM-specific cost store. | Stores costs in `litellm-costs.sqlite` under the configured SQLite home, deduplicated by thread/response ID. Missing historical costs cannot be reconstructed. |
| Service-tier status | No dedicated `service-tier` footer item. | Adds **Slow / Standard / Fast** status. This reflects client selection, not proof of the provider's final billing tier. |
| Response tier metadata | Completion events do not carry the additional tier field through the same internal path. | Propagates server-reported `service_tier` through completion events and logs it in debug metadata. |
| Guardian auto-review provider | Uses the normal provider selection path. | Adds optional **`[auto_review] model_provider = "..."`** for a separately configured Guardian provider. This does not enable automatic approvals by itself. |
| Packaged setup | Official installers distribute the upstream build. | The custom release includes both binaries and a GPT-6.1-Sol catalog exposing **`/slow`**. Guided setup adds a separate profile, personal Keychain access, and a shortcut that avoids existing names. |

Try Flex for codebase exploration, documentation, planned refactors, and other work that can tolerate a wait. For time-sensitive tasks, choose Standard. The model stays the same when toggling tiers; scheduling and billing depend on your provider.

**Savings depend on actual gateway pricing and eligible usage.** No fixed discount or measured company-wide savings is established here. Flex can be slower or unavailable. Displayed costs are reported usage data, not reconciled invoices, and are omitted when the gateway does not supply them.

## Install: Apple Silicon Mac + zsh

You need an Apple Silicon Mac, **zsh**, network access to your LiteLLM gateway, and **your own LiteLLM API key**. The download is approximately 149 MB. It was smoke-tested on macOS 15.6. This release has no Intel Mac, Linux, or Windows binaries.

### Prefer to use an existing agent?

Paste this into your agent. Traveloka colleagues can use the gateway-filled version in the internal launch guide.

```text
Install Codex LiteLLM using this setup script:
https://raw.githubusercontent.com/zjsng-trav/codex/main/scripts/install-codex-litellm.sh

Ask me for my LiteLLM gateway URL and pass it using --gateway. Preserve my existing codex command and profiles. Let the script choose an unused shortcut and handle Keychain setup. If it needs my key, have me enter it privately in Terminal, never in chat.

Verify the shortcut with --version and tell me the command to use.
```

### 1. Run guided setup

Read the [setup script](scripts/install-codex-litellm.sh), then paste this into Terminal:

```sh
setup_script="$(mktemp -t codex-litellm-setup)"
curl --proto '=https' --tlsv1.2 -fL \
  https://raw.githubusercontent.com/zjsng-trav/codex/main/scripts/install-codex-litellm.sh \
  -o "$setup_script"
bash "$setup_script"
```

It asks for your **LiteLLM HTTPS base URL**, including `/v1`, and your personal key if one is not already available. Key input is hidden. Ask your gateway administrator for access to **`gpt-6.1-sol`** and Flex processing. Traveloka colleagues should use the internal launch guide for the shared gateway URL.

You can supply the gateway up front, or add `--dry-run` to preview the chosen paths and shortcut:

```sh
bash "$setup_script" --gateway https://YOUR-LITELLM-GATEWAY/v1 --dry-run
bash "$setup_script" --gateway https://YOUR-LITELLM-GATEWAY/v1
```

Replace the example URL before running these commands.

### 2. Open a new terminal and use the printed shortcut

Normally:

```sh
codex-litellm
```

If that name is occupied, setup tries **`codex-litellm-jet`**, then **`codex-litellm-jet-2`**, and so on. It checks configured aliases, functions, executable commands, and profile/launcher paths. **Use the exact name printed by setup.** Transient aliases defined only in an already-open shell are not visible to a child setup process; a new terminal loads the checked configuration.

The official **`codex`** command is preserved. No global npm package, Homebrew installation, or existing Codex profile is replaced.

### 3. Confirm Slow mode and try a task

The generated profile starts with **`service_tier = "flex"`**, so the footer should show **Slow**. Try a small codebase question. **`/slow` toggles Slow and Standard**; check the footer afterward. Press **Esc** to stop waiting for Flex capacity.

The profile includes the `estimated-thread-cost` footer item. An absent cost means the gateway has not supplied a usable cost; it does not mean the request was free.

## What setup changes

| Location | Purpose |
| --- | --- |
| `~/.local/share/codex-litellm/0.166.0/` | Versioned binaries, model catalog, and launcher. |
| `$CODEX_HOME/<shortcut>.config.toml`, or `~/.codex/<shortcut>.config.toml` | Separate named profile; existing profiles are preserved. |
| Your zsh configuration, normally `~/.zshrc` | Adds a named alias. Backs up an existing file before editing it; respects `ZDOTDIR`. |
| macOS Keychain: service `codex-litellm`, account `$USER` | Stores your personal key when needed; the launcher reads it at runtime. |

If **`CODEX_LITELLM_API_KEY`** is already set, it takes precedence and setup does not copy it into Keychain. Otherwise, setup reuses an existing Keychain item or asks for your key. No key is written to shell configuration, the profile, repository, or package. To replace a stored key, update that service/account item in **Keychain Access**.

Rerunning setup reuses its matching profile and shortcut. Conflicting names or changed profiles cause it to choose another available name. The launcher uses **`--no-daemon`**, ensuring this session runs the custom app server instead of an existing upstream daemon. Base and project configuration still apply underneath the profile.

## Configuration details

Setup generates the v0.166.0 named-file format, **`<shortcut>.config.toml`**, rather than legacy `[profiles.*]` tables. You do not need to create this manually:

```toml
model_provider = "litellm"
model = "gpt-6.1-sol"
model_reasoning_effort = "high"
service_tier = "flex"
# Setup writes the absolute path for your installation here.
model_catalog_json = "/YOUR/INSTALL/PREFIX/libexec/codex-litellm/litellm-models.json"

[model_providers.litellm]
name = "LiteLLM"
base_url = "https://YOUR-LITELLM-GATEWAY/v1"
env_key = "CODEX_LITELLM_API_KEY"
wire_api = "responses"
requires_openai_auth = false

[tui]
status_line = ["model-with-reasoning", "service-tier", "project-name", "context-used", "estimated-thread-cost"]
```

**Keep provider ID `litellm`** for the custom cost and retry paths. The packaged catalog exposes `/slow` for `gpt-6.1-sol`; catalog metadata does not grant model or tier access. Your gateway must accept and honor Flex requests. Other models require corresponding catalog entries and provider support.

To disable the special unbounded Flex retries, add **`stream_max_retries = 0`** inside the existing `[model_providers.litellm]` table.

Guardian provider selection is optional: `[auto_review]` can contain `model_provider = "litellm"`. Normal approval and sandbox settings still govern review behavior; setup does not enable automatic approvals.

## Troubleshooting, updates, and removal

| Symptom | Next step |
| --- | --- |
| Shortcut not found | Open a new terminal and use the printed name. Custom shell layouts can pass `--shell-rc /absolute/path`. |
| Existing installation or profile | Matching managed configuration is reused; conflicts are skipped. `--prefix /absolute/path` selects another installation root. |
| Missing key or HTTP 401/403 | Check your own Keychain item or `CODEX_LITELLM_API_KEY`, gateway access, and model permissions. |
| `/slow` missing | Use the generated shortcut/profile and its catalog. Another CLI or catalog may expose different commands. |
| Repeated capacity messages | Wait, cancel with Esc, or deliberately switch to Standard. |
| No cost shown | Check that LiteLLM returns `usage.cost` in USD. Older/missing records cannot be backfilled. |
| macOS blocks execution | Binaries are ad-hoc signed, not Apple-notarized. Verify the download and use your organization's approved installation process. |

**Updates:** use releases from **this fork** and matching setup instructions. Official installers and the upstream updater distribute upstream binaries and do not preserve these changes. The current setup script pins v0.166.0 and the archive SHA-256.

**Removal:** remove the two-line Codex LiteLLM alias block from your zsh configuration, delete its matching named profile, and remove the versioned installation directory after closing its sessions. Delete the Keychain item only if no longer needed. The official `codex` installation remains usable.

## Source and verification

The release contains `BUILD_INFO`, checksums, [Apache-2.0 LICENSE](LICENSE), and upstream [NOTICE](NOTICE). Its binaries matched the recorded SHA-256 values. Package installation, CLI version/help, and refusal to overwrite an existing installation were smoke-tested. The guided setup was also smoke-tested against an existing command and alias, a path containing spaces, repeat runs, and offline profile/catalog loading. Source tests cover Flex retries, cost persistence, and UI snapshots. This setup/documentation change does not rebuild or rerun the Rust suite.

For other platforms, use this fork's source and pinned Rust toolchain. [Upstream build guidance](docs/install.md) describes the process; substitute **`https://github.com/zjsng-trav/codex.git`** when cloning. Other platforms have not been validated for this custom release.

General usage: [official Codex documentation](https://developers.openai.com/codex). Fork-specific feedback: [GitHub issues](https://github.com/zjsng-trav/codex/issues). Never include API keys in an issue or screenshot.
