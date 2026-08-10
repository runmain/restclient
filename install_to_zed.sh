#!/usr/bin/env bash
# Build and install the HTTPyac Client extension for Zed (macOS + Linux)
set -euo pipefail

SCRIPT_SOURCE="${BASH_SOURCE[0]}"
while [[ -L "$SCRIPT_SOURCE" ]]; do
  SCRIPT_DIR="$(cd -P "$(dirname "$SCRIPT_SOURCE")" && pwd)"
  SCRIPT_SOURCE="$(readlink "$SCRIPT_SOURCE")"
  [[ "$SCRIPT_SOURCE" != /* ]] && SCRIPT_SOURCE="$SCRIPT_DIR/$SCRIPT_SOURCE"
done
PROJECT_DIR="$(cd -P "$(dirname "$SCRIPT_SOURCE")" && pwd)"
cd "$PROJECT_DIR"

if [[ -z "${HOME:-}" ]]; then
  echo "ERROR: HOME is not set; unable to determine the Zed and local binary installation directories."
  exit 2
fi

# In containers, explicitly target host-mounted directories for the system and architecture running Zed.
ZED_EXT_DIR_OVERRIDE="${ZED_EXT_DIR:-}"
LOCAL_BIN_OVERRIDE="${HTTPYAC_LOCAL_BIN:-}"
INSTALL_DIR=""
INSTALL_STAGE=""
INSTALL_BACKUP=""
GRAMMAR_TMP=""

cleanup_install() {
  if [[ -n "${INSTALL_STAGE:-}" && -d "$INSTALL_STAGE" ]]; then
    rm -rf "$INSTALL_STAGE"
  fi
  if [[ -n "${INSTALL_BACKUP:-}" && -d "$INSTALL_BACKUP" && -n "${INSTALL_DIR:-}" && ! -e "$INSTALL_DIR" ]]; then
    mv "$INSTALL_BACKUP" "$INSTALL_DIR" || true
  fi
  if [[ -n "${GRAMMAR_TMP:-}" && -d "$GRAMMAR_TMP" ]]; then
    rm -rf "$GRAMMAR_TMP"
  fi
}
trap cleanup_install EXIT

# Installation status flags for the final summary.
OK_LSP=0
OK_WASM=0
OK_GRAMMAR=0
OK_EXT=0
OK_HTTPYAC=0
OK_VTSLS=0
OK_SETTINGS="Skipped"
VTSLS_PATH="vtsls"
HTTPYAC_PATH="httpyac"
LSP_PATH=""
WARNINGS=()
# User actions shown in the final required/optional checklist.
MANUAL_REQUIRED=()   # Skipping these may affect functionality.
MANUAL_OPTIONAL=()   # Optional improvements.

# Disable terminal colors when stdout is not a TTY.
if [[ -t 1 ]] && [[ "${NO_COLOR:-}" == "" ]]; then
  C_RESET=$'\033[0m'
  C_BOLD=$'\033[1m'
  C_DIM=$'\033[2m'
  C_RED=$'\033[1;31m'
  C_YEL=$'\033[1;33m'
  C_GRN=$'\033[1;32m'
  C_CYN=$'\033[1;36m'
  C_MAG=$'\033[1;35m'
  C_BG_YEL=$'\033[1;30;43m'   # Black text on yellow: attention or copy block.
  C_BG_RED=$'\033[1;37;41m'   # White text on red: required action.
  C_BG_CYN=$'\033[1;30;46m'   # Black text on cyan: optional action or note.
else
  C_RESET="" C_BOLD="" C_DIM="" C_RED="" C_YEL="" C_GRN="" C_CYN="" C_MAG=""
  C_BG_YEL="" C_BG_RED="" C_BG_CYN=""
fi

# Highlight a required manual action.
print_manual_box() {
  local title="$1"
  shift
  echo ""
  echo "${C_BG_RED}+----------------------------------------------------------------+${C_RESET}"
  echo "${C_BG_RED}| Manual action required (read carefully)                        |${C_RESET}"
  echo "${C_BG_RED}+----------------------------------------------------------------+${C_RESET}"
  echo "${C_YEL}${C_BOLD}>>> $title${C_RESET}"
  echo "${C_YEL}------------------------------------------------------------------${C_RESET}"
  while [[ $# -gt 0 ]]; do
    if [[ -z "$1" ]]; then
      echo ""
    else
      echo "${C_YEL}  - ${C_RESET}$1"
    fi
    shift
  done
  echo "${C_YEL}------------------------------------------------------------------${C_RESET}"
  echo ""
}

# Highlight the settings block to copy.
print_manual_json_block() {
  local settings_path="$1"
  echo ""
  echo "${C_BG_YEL}********************************************************************${C_RESET}"
  echo "${C_BG_YEL}**   Copy this entire block into settings.json (merge it)       **${C_RESET}"
  echo "${C_BG_YEL}********************************************************************${C_RESET}"
  echo ""
  echo "${C_CYN}${C_BOLD}  Target file${C_RESET}"
  echo "    ${C_BOLD}$settings_path${C_RESET}"
  echo ""
  echo "${C_CYN}${C_BOLD}  Steps${C_RESET}"
  echo "    1. Open the settings.json file above in an editor."
  echo "    2. ${C_YEL}${C_BOLD}Merge${C_RESET} the fields below into the top-level { ... } object without duplicate keys."
  echo "    3. Preserve JSON ${C_YEL}commas${C_RESET} and ${C_GRN}keep${C_RESET} your existing settings and comments."
  echo ""
  echo "${C_BG_YEL}>>>>>>>>>>  COPY FROM THE NEXT LINE  >>>>>>>>>>${C_RESET}"
  echo "${C_DIM}# ----- COPY START (Zed settings support // comments; automatic writes remove them) -----${C_RESET}"
  cat <<EOF
  // httpyac extension settings (use absolute paths for path values)
  "httpyac": {
    // command: httpyac CLI executable path or the command name "httpyac" in PATH
    "command": "$HTTPYAC_PATH",
    // lsp_command: httpyac-lsp path, usually ~/.local/bin/httpyac-lsp
    "lsp_command": "$LSP_PATH",
    // vtsls_command: child vtsls path for script blocks and official model completions, or "vtsls"
    "vtsls_command": "$VTSLS_PATH",
    // Child vtsls provides official models and Node/JS completions in script blocks.
    // default_env: a display/default environment name such as "dev", "prod", or "test"; requests use activeEnv from env JSON.
    "default_env": "dev"
  },
  "lsp": {
    "httpyac-lsp": {
      "binary": {
        // path: absolute path used by Zed to start the LSP
        "path": "$LSP_PATH",
        // arguments: additional arguments passed to httpyac-lsp, usually []
        "arguments": []
      },
      "settings": {
        // vtslsCommand: same as httpyac.vtsls_command; the LSP reads this first
        // Child vtsls provides official models and Node/JS completions in script blocks.
        "vtslsCommand": "$VTSLS_PATH"
      }
    }
  },
  "languages": {
    "HTTP": {
      // enable_language_server: true | false
      "enable_language_server": true,
      // language_servers: use only ["httpyac-lsp"]; do not add "vtsls" because it treats the full file as TypeScript
      "language_servers": ["httpyac-lsp"],
      "completions": {
        // lsp: true enables LSP completions; false disables them
        "lsp": true,
        // words: "disabled" disables word completions; "fallback" uses words only when LSP has no result; "enabled" always uses words
        // Use fallback or disabled to prevent "Hello" from hiding "Host".
        "words": "disabled"
        // Optional words_min_length: integer >= 1, with 3 as a common value
      }
    }
  }
EOF
  echo "${C_DIM}# ----- COPY END -----${C_RESET}"
  echo "${C_BG_YEL}<<<<<<<<<<  COPY THROUGH THE PREVIOUS LINE  <<<<<<<<<<${C_RESET}"
  echo ""
  echo "${C_CYN}${C_BOLD}  Configuration reference${C_RESET}"
  echo "    ${C_BOLD}Script completions (always mixed inside {{}})${C_RESET}"
  echo "      Child vtsls provides official httpyac globals and Node/JS script completions."
  echo "      vtslsCommand / httpyac.vtsls_command"
  echo "        -> absolute vtsls path or its command name in PATH ${C_DIM}(required for script completions)${C_RESET}"
  echo "    ${C_BOLD}Path values (strings)${C_RESET}"
  echo "      httpyac.command / lsp_command / vtsls_command / lsp.httpyac-lsp.binary.path"
  echo "        -> absolute path or its command name in PATH"
  echo "    ${C_BOLD}HTTP language${C_RESET}"
  echo "      language_servers -> only ${C_GRN}[\"httpyac-lsp\"]${C_RESET} (do not add vtsls)"
  echo "      completions.words -> ${C_GRN}\"disabled\"${C_RESET} | \"fallback\" | \"enabled\""
  echo "      completions.lsp   -> true | false"
  echo "      default_env       -> any environment name string (dev/prod/etc.)"
  echo ""
  echo "${C_MAG}${C_BOLD}  Prefer not to edit manually?${C_RESET} Force an automatic write (${C_RED}// comments will be removed${C_RESET}; a backup is created):"
  echo "    ${C_BOLD}FORCE_ZED_SETTINGS=1 $0${C_RESET}"
  echo ""
}

# ---------------------------------------------------------------------------
# Detect platform-specific paths without hard-coded machine paths.
# ---------------------------------------------------------------------------
detect_os_paths() {
  local config_home="${XDG_CONFIG_HOME:-$HOME/.config}"

  if [[ "${OSTYPE:-}" == darwin* ]]; then
    IS_MACOS=1
    # Extensions remain under Application Support on macOS.
    ZED_EXT_DIR="${ZED_EXT_DIR_OVERRIDE:-${HOME}/Library/Application Support/Zed/extensions/installed}"
    # Prefer ~/.config/zed/settings.json, the current Zed default.
    local mac_settings_xdg="${config_home}/zed/settings.json"
    local mac_settings_app="${HOME}/Library/Application Support/Zed/settings.json"
    if [[ -f "$mac_settings_xdg" ]]; then
      ZED_SETTINGS="$mac_settings_xdg"
    elif [[ -f "$mac_settings_app" ]]; then
      ZED_SETTINGS="$mac_settings_app"
    else
      ZED_SETTINGS="$mac_settings_xdg"
    fi
  else
    IS_MACOS=0
    local data_home="${XDG_DATA_HOME:-$HOME/.local/share}"
    local zed_data_ext="${data_home}/zed/extensions/installed"
    local zed_legacy_ext="${config_home}/zed/extensions/installed"
    if [[ -n "$ZED_EXT_DIR_OVERRIDE" ]]; then
      ZED_EXT_DIR="$ZED_EXT_DIR_OVERRIDE"
    elif [[ -d "$zed_data_ext" || ! -d "$zed_legacy_ext" ]]; then
      ZED_EXT_DIR="$zed_data_ext"
    else
      # Preserve the legacy config-directory installation when it already exists.
      ZED_EXT_DIR="$zed_legacy_ext"
    fi
    ZED_SETTINGS="${config_home}/zed/settings.json"
  fi

  LOCAL_BIN="${LOCAL_BIN_OVERRIDE:-${HOME}/.local/bin}"
}

is_container() {
  [[ -f /.dockerenv ]] || [[ -f /run/.containerenv ]] || \
    { [[ -r /proc/1/cgroup ]] && grep -qaE '(docker|containerd|kubepods|podman)' /proc/1/cgroup; }
}

require_command() {
  local command_name="$1"
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "ERROR: Missing required command: $command_name"
    exit 2
  fi
}

sign_macos_binary() {
  local binary_path="$1"
  if ! codesign --force --sign - "$binary_path" >/dev/null 2>&1; then
    echo "  WARNING: codesign failed: $binary_path"
    WARNINGS+=("macOS signing failed: $binary_path")
  fi
}

echo "=========================================="
echo " HTTPyac Client - Build and Install for Zed"
echo "=========================================="
echo ""

detect_os_paths
if is_container && { [[ -z "$ZED_EXT_DIR_OVERRIDE" ]] || [[ -z "$LOCAL_BIN_OVERRIDE" ]]; }; then
  print_manual_box "Container installations must explicitly specify host-mounted directories" \
    "The default HOME writes only inside the container, so the host Zed and Send task cannot use those files." \
    "Run this only when the container and the Zed host use the same operating system and CPU architecture:" \
    "  ZED_EXT_DIR=/mounted/Zed/extensions/installed HTTPYAC_LOCAL_BIN=/mounted/local/bin ./install_to_zed.sh" \
    "A macOS host cannot use Linux Docker artifacts; run this script directly on the macOS host."
  exit 2
fi
echo "Paths"
echo "  Extension directory: $ZED_EXT_DIR"
echo "  Settings file:       $ZED_SETTINGS"
echo "  Local bin directory: $LOCAL_BIN"
echo ""

# ---------------------------------------------------------------------------
# 1) Build httpyac-lsp.
# ---------------------------------------------------------------------------
require_command cargo
require_command rustup

echo "[1/7] Build httpyac-lsp + httpyac-run (release)..."
if (cd lsp && cargo build --release); then
  echo "  OK: httpyac-lsp built successfully"
  echo "  OK: httpyac-run built successfully (Send and environment selector entry point)"
  OK_LSP=1
else
  echo "  ERROR: LSP/task runner build failed"
  exit 1
fi
echo ""

# ---------------------------------------------------------------------------
# 2) Build the WASM extension as a Component Model target (wasm32-wasip2).
#    Zed loads extension.wasm with a component parser; wasip1 output is an MVP module and causes:
#    "attempted to parse a wasm module with a component parser"
#    -> extension load failure -> httpyac-lsp never starts -> only buffer-word completions such as "Hello"
# ---------------------------------------------------------------------------
echo "[2/7] Install the wasm32-wasip2 target if needed..."
if ! rustup target list --installed | grep -qx 'wasm32-wasip2'; then
  if ! rustup target add wasm32-wasip2; then
    echo "  ERROR: Unable to install wasm32-wasip2; check the rustup mirror, network, and toolchain."
    exit 1
  fi
fi

echo "[3/7] Build the WASM extension (wasm32-wasip2 component)..."
if cargo build --release --target wasm32-wasip2; then
  :
else
  echo "  ERROR: WASM extension build failed"
  exit 1
fi

WASM_SRC=""
for candidate in \
  "target/wasm32-wasip2/release/zed_httpyacclient.wasm" \
  "target/wasm32-wasip2/release/httpyacclient.wasm"
do
  if [[ -f "$candidate" ]]; then
    WASM_SRC="$candidate"
    break
  fi
done

if [[ -z "$WASM_SRC" ]]; then
  echo "  ERROR: WASM artifact not found (expected zed_httpyacclient.wasm)"
  ls -la target/wasm32-wasip2/release/ 2>/dev/null || true
  exit 1
fi

cp "$WASM_SRC" extension.wasm

# Validate the WebAssembly Component required by Zed (file shows version 0x1000d), not MVP 0x1.
WASM_FILE_INFO="$(file extension.wasm 2>/dev/null || true)"
if echo "$WASM_FILE_INFO" | grep -q "0x1 (MVP)"; then
  echo "  ERROR: extension.wasm is still an MVP module, not a Component:"
  echo "     $WASM_FILE_INFO"
  echo "     Use a newer rustc/cargo version and ensure --target wasm32-wasip2 is set."
  exit 1
fi
if echo "$WASM_FILE_INFO" | grep -qE "0x1000d|component"; then
  echo "  OK: extension.wasm is a Component <- $WASM_SRC ($(du -h extension.wasm | awk '{print $1}'))"
else
  echo "  WARNING: file(1) could not confirm the component format; continuing installation:"
  echo "     $WASM_FILE_INFO"
  echo "  OK: extension.wasm <- $WASM_SRC ($(du -h extension.wasm | awk '{print $1}'))"
fi
OK_WASM=1
echo ""
# ---------------------------------------------------------------------------
# 3) Grammar file.
# ---------------------------------------------------------------------------
echo "[4/7] Check the tree-sitter grammar..."
if [[ ! -f grammars/http.wasm ]]; then
  echo "  Building grammars/http.wasm..."
  require_command git
  require_command npm
  if ! command -v tree-sitter >/dev/null 2>&1; then
    echo "  Installing tree-sitter-cli with npm..."
    npm install -g tree-sitter-cli
  fi
  GRAMMAR_TMP="$(mktemp -d)"
  git clone --depth 1 https://github.com/rest-nvim/tree-sitter-http.git "$GRAMMAR_TMP/tree-sitter-http"
  (cd "$GRAMMAR_TMP/tree-sitter-http" && tree-sitter build --wasm -o http.wasm)
  mkdir -p grammars
  cp "$GRAMMAR_TMP/tree-sitter-http/http.wasm" grammars/http.wasm
  rm -rf "$GRAMMAR_TMP"
  GRAMMAR_TMP=""
  echo "  OK: grammars/http.wasm generated"
else
  echo "  OK: grammars/http.wasm already exists"
fi
OK_GRAMMAR=1
echo ""

# ---------------------------------------------------------------------------
# 4) Install into the Zed extension directory.
# ---------------------------------------------------------------------------
echo "[5/7] Install extension files into Zed..."
mkdir -p "$ZED_EXT_DIR"
INSTALL_DIR="${ZED_EXT_DIR}/httpyacclient"
INSTALL_STAGE="$(mktemp -d "$ZED_EXT_DIR/.httpyacclient.stage.XXXXXX")"
mkdir -p "$INSTALL_STAGE/grammars" "$INSTALL_STAGE/lsp"

cp extension.toml extension.wasm "$INSTALL_STAGE/"
cp -R languages "$INSTALL_STAGE/"
cp grammars/http.wasm "$INSTALL_STAGE/grammars/"
if [[ -d snippets ]]; then
  cp -R snippets "$INSTALL_STAGE/"
fi
cp lsp/target/release/httpyac-lsp "$INSTALL_STAGE/lsp/httpyac-lsp"
cp lsp/target/release/httpyac-run "$INSTALL_STAGE/lsp/httpyac-run"
mkdir -p "$INSTALL_STAGE/lsp/httpyac-models"
cp lsp/httpyac-models/httpyac-globals.d.ts "$INSTALL_STAGE/lsp/httpyac-models/httpyac-globals.d.ts"
cp -R lsp/httpyac-models/src "$INSTALL_STAGE/lsp/httpyac-models/"
chmod +x "$INSTALL_STAGE/lsp/httpyac-lsp" "$INSTALL_STAGE/lsp/httpyac-run"

if [[ "$IS_MACOS" -eq 1 ]]; then
  sign_macos_binary "$INSTALL_STAGE/lsp/httpyac-lsp"
  sign_macos_binary "$INSTALL_STAGE/lsp/httpyac-run"
fi

if [[ ! -f "$INSTALL_STAGE/extension.wasm" || ! -f "$INSTALL_STAGE/extension.toml" \
   || ! -f "$INSTALL_STAGE/grammars/http.wasm" || ! -x "$INSTALL_STAGE/lsp/httpyac-lsp" ]]; then
  echo "  ERROR: Staged extension is incomplete; existing installation was preserved."
  exit 1
fi

if [[ -e "$INSTALL_DIR" ]]; then
  INSTALL_BACKUP="${INSTALL_DIR}.backup.$$"
  mv "$INSTALL_DIR" "$INSTALL_BACKUP"
fi
if ! mv "$INSTALL_STAGE" "$INSTALL_DIR"; then
  echo "  ERROR: Extension replacement failed; restoring the previous version."
  exit 1
fi
INSTALL_STAGE=""
if [[ -n "$INSTALL_BACKUP" && -d "$INSTALL_BACKUP" ]]; then
  rm -rf "$INSTALL_BACKUP"
  INSTALL_BACKUP=""
fi

mkdir -p "$LOCAL_BIN"
cp lsp/target/release/httpyac-lsp "$LOCAL_BIN/httpyac-lsp"
cp lsp/target/release/httpyac-run "$LOCAL_BIN/httpyac-run"
mkdir -p "$LOCAL_BIN/httpyac-models"
cp lsp/httpyac-models/httpyac-globals.d.ts "$LOCAL_BIN/httpyac-models/httpyac-globals.d.ts"
cp -R lsp/httpyac-models/src "$LOCAL_BIN/httpyac-models/"
chmod +x "$LOCAL_BIN/httpyac-lsp" "$LOCAL_BIN/httpyac-run"
if [[ "$IS_MACOS" -eq 1 ]]; then
  sign_macos_binary "$LOCAL_BIN/httpyac-lsp"
  sign_macos_binary "$LOCAL_BIN/httpyac-run"
fi

# Verify that all required files are in place.
if [[ -f "$INSTALL_DIR/extension.wasm" && -f "$INSTALL_DIR/extension.toml" \
   && -f "$INSTALL_DIR/grammars/http.wasm" && -x "$INSTALL_DIR/lsp/httpyac-lsp" \
   && -x "$LOCAL_BIN/httpyac-lsp" && -x "$LOCAL_BIN/httpyac-run" ]]; then
  echo "  OK: Extension installed -> $INSTALL_DIR"
  echo "  OK: LSP installed -> $LOCAL_BIN/httpyac-lsp"
  echo "  OK: Send entry point -> $LOCAL_BIN/httpyac-run (Send / environment selector)"
  OK_EXT=1
else
  echo "  ERROR: Extension installation is incomplete; check the paths above."
  exit 1
fi
echo ""

# ---------------------------------------------------------------------------
# 5) Locate httpyac, httpyac-lsp, and vtsls.
# ---------------------------------------------------------------------------
echo "[6/7] Locate httpyac, httpyac-lsp, and vtsls..."
HTTPYAC_PATH="$(command -v httpyac 2>/dev/null || true)"
if [[ -z "$HTTPYAC_PATH" ]]; then
  HTTPYAC_PATH="httpyac"
  OK_HTTPYAC=0
  print_manual_box "Install the httpyac CLI (required to send requests)" \
    "Reason: the httpyac command is not in the current PATH." \
    "" \
    "Run:" \
    "  npm install -g httpyac" \
    "" \
    "Then verify:" \
    "  which httpyac && httpyac --version"
  WARNINGS+=("httpyac CLI was not found")
  MANUAL_REQUIRED+=("Install httpyac: npm install -g httpyac")
else
  echo "  OK: httpyac     = $HTTPYAC_PATH"
  OK_HTTPYAC=1
fi

LSP_PATH="$LOCAL_BIN/httpyac-lsp"
if [[ -x "$LOCAL_BIN/httpyac-lsp" ]]; then
  LSP_PATH="$LOCAL_BIN/httpyac-lsp"
elif command -v httpyac-lsp >/dev/null 2>&1; then
  LSP_PATH="$(command -v httpyac-lsp)"
fi
echo "  OK: httpyac-lsp = $LSP_PATH"
if [[ -x "$LOCAL_BIN/httpyac-run" ]]; then
  echo "  OK: httpyac-run = $LOCAL_BIN/httpyac-run"
else
  echo "  WARNING: httpyac-run is not installed"
  WARNINGS+=("httpyac-run is missing; the Send button may fail")
fi

# vtsls = @vtsls/language-server for full TypeScript/Node IntelliSense in script blocks.
OK_VTSLS=0
VTSLS_PATH="$(command -v vtsls 2>/dev/null || true)"
if [[ -z "$VTSLS_PATH" ]]; then
  # common npm global layouts
  for cand in \
    "${NPM_CONFIG_PREFIX:-}/bin/vtsls" \
    "$(npm root -g 2>/dev/null)/../bin/vtsls" \
    "$HOME/.local/bin/vtsls"
  do
    if [[ -n "$cand" && -x "$cand" ]]; then
      VTSLS_PATH="$cand"
      break
    fi
  done
fi
if [[ -z "$VTSLS_PATH" ]]; then
  VTSLS_PATH="vtsls"
  print_manual_box "Install vtsls (recommended for Node/JS script completions)" \
    "Reason: vtsls (@vtsls/language-server) is not in the current PATH." \
    "Details: vtsls provides official httpyac globals plus request/response and crypto/Node completions in script blocks." \
    "" \
    "Run:" \
    "  npm install -g @vtsls/language-server" \
    "" \
    "Then verify:" \
    "  which vtsls && vtsls --version" \
    "" \
    "After installation, set this in settings:" \
    "  \"vtslsCommand\": \"\$(which vtsls)\"   // Path string; enables mixed Node/JS completions when available"
  WARNINGS+=("vtsls was not found; official models and Node/JS script completions are unavailable")
  MANUAL_OPTIONAL+=("Install npm i -g @vtsls/language-server to enable official models and Node/JS script completions")
else
  echo "  OK: vtsls      = $VTSLS_PATH"
  OK_VTSLS=1
fi
echo ""

case ":${PATH}:" in
  *":${LOCAL_BIN}:"*) ;;
  *)
    print_manual_box "Add ~/.local/bin to PATH (recommended and required to send requests)" \
      "Reason: $LOCAL_BIN is not in the current PATH." \
      "Details: Zed Send invokes httpyac-run, which must be discoverable through PATH." \
      "" \
      "Add this line for your shell:" \
      "  export PATH=\"\$HOME/.local/bin:\$PATH\"" \
      "" \
      "fish: fish_add_path \$HOME/.local/bin; zsh/bash: source ~/.zshrc or source ~/.bashrc" \
      "Then fully quit and restart Zed."
    WARNINGS+=("\$HOME/.local/bin is not in PATH; Send may not find httpyac-run")
    MANUAL_REQUIRED+=("Add \$HOME/.local/bin to PATH and restart Zed, or Send may fail")
    ;;
esac

# ---------------------------------------------------------------------------
# 6) settings.json (do not overwrite comments by default).
# ---------------------------------------------------------------------------
echo "[7/7] Process Zed settings.json..."
if [[ "${SKIP_ZED_SETTINGS:-0}" == "1" ]]; then
  echo "  SKIPPED: SKIP_ZED_SETTINGS=1 is set; settings were not modified"
  OK_SETTINGS="Skipped by request"
  MANUAL_OPTIONAL+=("Edit settings manually if needed: $ZED_SETTINGS")
elif [[ "${FORCE_ZED_SETTINGS:-0}" != "1" ]] && [[ -f "$ZED_SETTINGS" ]] && \
     grep -qE '^\s*//|/\*' "$ZED_SETTINGS" 2>/dev/null; then
  print_manual_box "Merge Zed settings.json (not written automatically)" \
    "Reason: the configuration file contains // comments, and automatic writes would remove them." \
    "File: $ZED_SETTINGS" \
    "" \
    "Details: the extension is installed and usually works without settings;" \
    "         configuration pins absolute httpyac and LSP paths for greater reliability." \
    "" \
    "Open this file and merge the JSON block below into the top-level { } object."
  print_manual_json_block "$ZED_SETTINGS"
  OK_SETTINGS="Not written (comments require manual merge)"
  WARNINGS+=("settings were not written automatically")
  MANUAL_OPTIONAL+=("Manually merge settings.json using the manual action box above")
else
  mkdir -p "$(dirname "$ZED_SETTINGS")"
  if command -v python3 >/dev/null 2>&1; then
    SET_OUT="$(
    HTTPYAC_PATH="$HTTPYAC_PATH" LSP_PATH="$LSP_PATH" VTSLS_PATH="$VTSLS_PATH" \
      FORCE_ZED_SETTINGS="${FORCE_ZED_SETTINGS:-0}" \
      python3 - "$ZED_SETTINGS" <<'PY'
import json, os, re, shutil, sys, time

path = sys.argv[1]
httpyac = os.environ.get("HTTPYAC_PATH", "httpyac")
lsp = os.environ.get("LSP_PATH", os.path.expanduser("~/.local/bin/httpyac-lsp"))
vtsls = os.environ.get("VTSLS_PATH", "vtsls")
force = os.environ.get("FORCE_ZED_SETTINGS", "0") == "1"

existed = os.path.isfile(path) and os.path.getsize(path) > 0
if existed:
    raw = open(path, encoding="utf-8").read()
    has_comments = bool(
        re.search(r"(?m)^\s*//", raw)
        or re.search(r"/\*", raw)
        or re.search(r'(?<!:)//[^\n"]*$', raw, re.M)
    )
    if has_comments and not force:
        print("SKIP_COMMENTS")
        sys.exit(0)
    cleaned = re.sub(r"(?m)^\s*//.*$", "", raw)
    cleaned = re.sub(r'(?<!:)//[^\n"]*$', "", cleaned, flags=re.M)
    cleaned = re.sub(r"/\*.*?\*/", "", cleaned, flags=re.S)
    cleaned = re.sub(r",(\s*[}\]])", r"\1", cleaned).strip() or "{}"
    try:
        settings = json.loads(cleaned)
    except json.JSONDecodeError as e:
        print(f"PARSE_ERROR:{e}")
        sys.exit(0)
else:
    settings = {}

changed = []

def setp(keys, value, *, force_update=False):
    cur = settings
    for k in keys[:-1]:
        if k not in cur or not isinstance(cur[k], dict):
            cur[k] = {}
        cur = cur[k]
    last = keys[-1]
    miss = object()
    old = cur.get(last, miss)
    if old is not miss and old == value:
        return
    if force_update or last not in cur:
        cur[last] = value
        changed.append(".".join(keys))

# Prefer LSP completions over buffer-word noise (e.g. "Hello" hiding "Host")
setp(["languages", "HTTP", "completions", "words"], "fallback", force_update=True)
setp(["languages", "HTTP", "completions", "words_min_length"], 3, force_update=True)
setp(["httpyac", "command"], httpyac, force_update=True)
setp(["httpyac", "lsp_command"], lsp, force_update=True)
setp(["httpyac", "vtsls_command"], vtsls, force_update=True)
setp(["httpyac", "default_env"], "dev", force_update=False)
setp(["lsp", "httpyac-lsp", "binary", "path"], lsp, force_update=True)
setp(["lsp", "httpyac-lsp", "binary", "arguments"], [], force_update=False)
setp(["lsp", "httpyac-lsp", "settings", "vtslsCommand"], vtsls, force_update=True)
# Drop removed engine-toggle keys from older installs (always mixed now)
for _dead in (
    "useBuiltinScriptCompletions",
    "scriptCompletionSource",
    "use_builtin_script_completions",
    "script_completion_source",
    "vtslsEnabled",
    "vtsls_enabled",
):
    try:
        settings.get("lsp", {}).get("httpyac-lsp", {}).get("settings", {}).pop(_dead, None)
        settings.get("httpyac", {}).pop(_dead, None)
    except Exception:
        pass

if not changed:
    print("UP_TO_DATE")
    sys.exit(0)

if existed:
    bak = f"{path}.backup.{time.strftime('%Y%m%d%H%M%S')}"
    shutil.copy2(path, bak)
    print(f"BACKUP:{bak}")

with open(path, "w", encoding="utf-8") as f:
    json.dump(settings, f, indent=2, ensure_ascii=False)
    f.write("\n")

print("MERGED:" + ",".join(changed))
if force:
    print("FORCE_NO_COMMENTS")
PY
    )"
    case "$SET_OUT" in
      *SKIP_COMMENTS*)
        print_manual_box "Merge Zed settings.json (comments detected; not written automatically)" \
          "File: $ZED_SETTINGS" \
          "Manually merge the JSON block below."
        print_manual_json_block "$ZED_SETTINGS"
        OK_SETTINGS="Not written (comments require manual merge)"
        WARNINGS+=("settings were not written automatically")
        MANUAL_OPTIONAL+=("Manually merge settings.json using the box above")
        ;;
      *PARSE_ERROR*)
        print_manual_box "Fix settings.json and retry" \
          "Reason: unable to parse $ZED_SETTINGS" \
          "Check JSON/JSONC syntax or add the httpyac configuration manually."
        print_manual_json_block "$ZED_SETTINGS"
        OK_SETTINGS="Not written (parse failure requires manual action)"
        WARNINGS+=("settings parsing failed")
        MANUAL_OPTIONAL+=("Fix and merge settings.json")
        ;;
      *UP_TO_DATE*)
        echo "  OK: settings are already up to date; no write is needed"
        OK_SETTINGS="Ready (no changes needed)"
        ;;
      *MERGED:*)
        if [[ "$SET_OUT" == *BACKUP:* ]]; then
          echo "  Backup created: $(echo "$SET_OUT" | tr ' ' '\n' | grep '^BACKUP:' | head -1 | cut -d: -f2-)"
        fi
        echo "  OK: merged into settings.json without duplicate keys"
        OK_SETTINGS="Written (no further action needed)"
        if [[ "$SET_OUT" == *FORCE_NO_COMMENTS* ]]; then
          echo "  WARNING: forced overwrite removed existing // comments"
        fi
        ;;
      *)
        echo "  $SET_OUT"
        OK_SETTINGS="Processed"
        ;;
    esac
  else
    print_manual_box "Install python3 or edit settings manually" \
      "Reason: python3 was not found, so settings cannot be merged automatically." \
      "File: $ZED_SETTINGS"
    print_manual_json_block "$ZED_SETTINGS"
    OK_SETTINGS="Not written (python3 is required for automatic merge)"
    WARNINGS+=("python3 is unavailable, so settings were not changed")
    MANUAL_OPTIONAL+=("Manually merge settings.json or install python3 and run the script again")
  fi
fi

# ---------------------------------------------------------------------------
# Final summary.
# ---------------------------------------------------------------------------
echo ""
echo "=========================================="
echo "           Installation Summary"
echo "=========================================="
echo ""

CORE_OK=1
[[ "$OK_LSP" -eq 1 ]]     || CORE_OK=0
[[ "$OK_WASM" -eq 1 ]]    || CORE_OK=0
[[ "$OK_GRAMMAR" -eq 1 ]] || CORE_OK=0
[[ "$OK_EXT" -eq 1 ]]     || CORE_OK=0

if [[ "$CORE_OK" -eq 1 ]]; then
  echo "Result: Extension installation succeeded"
else
  echo "Result: Extension installation failed (a core step failed)"
fi
echo ""
echo "  Checks:"
echo "  - httpyac-lsp build:      $([ "$OK_LSP" -eq 1 ] && echo 'OK' || echo 'FAILED')"
echo "  - WASM extension build:   $([ "$OK_WASM" -eq 1 ] && echo 'OK' || echo 'FAILED')"
echo "  - Grammar file:           $([ "$OK_GRAMMAR" -eq 1 ] && echo 'OK' || echo 'FAILED')"
echo "  - Zed extension install:  $([ "$OK_EXT" -eq 1 ] && echo 'OK' || echo 'FAILED')"
echo "  - httpyac CLI:            $([ "$OK_HTTPYAC" -eq 1 ] && echo 'FOUND' || echo 'MISSING (install before sending requests)')"
echo "  - vtsls (script TS):      $([ "${OK_VTSLS:-0}" -eq 1 ] && echo 'FOUND' || echo 'MISSING (install for full script completions)')"
echo "  - settings.json:          $OK_SETTINGS"
echo ""
echo "  Installation paths:"
echo "  - Extension: $INSTALL_DIR"
echo "  - LSP:       $LSP_PATH"
echo "  - httpyac:   $HTTPYAC_PATH"
echo "  - vtsls:     $VTSLS_PATH"
echo ""

if [[ ${#WARNINGS[@]} -gt 0 ]]; then
  echo "  Warnings:"
  for w in "${WARNINGS[@]}"; do
    echo "  WARNING: $w"
  done
  echo ""
fi

# Manual action checklist.
echo ""
echo "${C_BG_RED}+----------------------------------------------------------------+${C_RESET}"
echo "${C_BG_RED}| Manual actions to complete                                     |${C_RESET}"
echo "${C_BG_RED}+----------------------------------------------------------------+${C_RESET}"
echo ""
HAS_MANUAL=0
if [[ ${#MANUAL_REQUIRED[@]} -gt 0 ]]; then
  HAS_MANUAL=1
  echo "${C_BG_YEL}  Required: skipping these may affect requests or usage. ${C_RESET}"
  local_i=1
  for item in "${MANUAL_REQUIRED[@]}"; do
    echo "    ${C_RED}${C_BOLD}$local_i.${C_RESET} ${C_BOLD}$item${C_RESET}"
    local_i=$((local_i + 1))
  done
  echo ""
fi
if [[ ${#MANUAL_OPTIONAL[@]} -gt 0 ]]; then
  HAS_MANUAL=1
  echo "${C_BG_CYN}  Optional: the extension usually works without these, but they are recommended. ${C_RESET}"
  local_i=1
  for item in "${MANUAL_OPTIONAL[@]}"; do
    echo "    ${C_CYN}$local_i.${C_RESET} $item"
    local_i=$((local_i + 1))
  done
  echo ""
fi
# Restarting Zed is always required after installing the extension.
if [[ "$CORE_OK" -eq 1 ]]; then
  HAS_MANUAL=1
  echo "${C_BG_YEL}  Required immediately after installation: ${C_RESET}"
  echo "    ${C_RED}${C_BOLD}1.${C_RESET} ${C_BOLD}Fully quit Zed (macOS: Cmd+Q, not only the window) and reopen it.${C_RESET}"
  echo "    ${C_RED}${C_BOLD}2.${C_RESET} ${C_BOLD}Open examples/basic.http and test Send from the left side of a request.${C_RESET}"
  if [[ "$OK_HTTPYAC" -eq 1 ]]; then
    echo "    ${C_DIM}3.${C_RESET} (Optional) Open examples/environments.http and use Env:* to switch environments."
  fi
  echo ""
fi
if [[ "$HAS_MANUAL" -eq 0 ]]; then
  echo "  No additional manual actions are required."
  echo ""
fi

if [[ "$CORE_OK" -eq 1 && "$OK_HTTPYAC" -eq 1 ]]; then
  echo "${C_BG_YEL}!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!${C_RESET}"
  echo "${C_GRN}${C_BOLD}Result: Extension installation succeeded${C_RESET}"
  echo "         Complete the required actions highlighted in yellow or red before use."
  echo "${C_BG_YEL}!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!${C_RESET}"
  echo ""
  exit 0
elif [[ "$CORE_OK" -eq 1 ]]; then
  echo "${C_BG_RED}!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!${C_RESET}"
  echo "${C_YEL}${C_BOLD}Result: Extension is installed, but the Send runtime is not ready${C_RESET}"
  echo "         Install httpyac and run this script again, or ensure Zed can find httpyac through PATH."
  echo "${C_BG_RED}!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!${C_RESET}"
  echo ""
  exit 2
else
  echo "${C_BG_RED}!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!${C_RESET}"
  echo "${C_RED}${C_BOLD}Result: Installation did not complete${C_RESET}"
  echo "         Review the errors and manual actions above, then retry."
  echo "${C_BG_RED}!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!${C_RESET}"
  echo ""
  exit 1
fi
