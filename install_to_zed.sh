#!/usr/bin/env bash
# 编译并安装 HTTPyac Client 扩展到 Zed（macOS + Linux）
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$PROJECT_DIR"

# 安装结果标记（用于最终结论）
OK_LSP=0
OK_WASM=0
OK_GRAMMAR=0
OK_EXT=0
OK_HTTPYAC=0
OK_VTSLS=0
OK_SETTINGS="跳过"
VTSLS_PATH="vtsls"
HTTPYAC_PATH="httpyac"
LSP_PATH=""
WARNINGS=()
# 需要用户手动操作的条目（最终「必做/选做」清单）
MANUAL_REQUIRED=()   # 不处理可能影响使用
MANUAL_OPTIONAL=()   # 可选优化

# 终端颜色（非 TTY 时关闭，避免日志乱码）
if [[ -t 1 ]] && [[ "${NO_COLOR:-}" == "" ]]; then
  C_RESET=$'\033[0m'
  C_BOLD=$'\033[1m'
  C_DIM=$'\033[2m'
  C_RED=$'\033[1;31m'
  C_YEL=$'\033[1;33m'
  C_GRN=$'\033[1;32m'
  C_CYN=$'\033[1;36m'
  C_MAG=$'\033[1;35m'
  C_BG_YEL=$'\033[1;30;43m'   # 黑字黄底 — 注意/请复制
  C_BG_RED=$'\033[1;37;41m'   # 白字红底 — 必做
  C_BG_CYN=$'\033[1;30;46m'   # 黑字青底 — 选做/提示
else
  C_RESET="" C_BOLD="" C_DIM="" C_RED="" C_YEL="" C_GRN="" C_CYN="" C_MAG=""
  C_BG_YEL="" C_BG_RED="" C_BG_CYN=""
fi

# 高亮「需要注意」一行条幅
print_attention() {
  # 用法: print_attention "标题文字"
  echo ""
  echo "${C_BG_YEL}!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!${C_RESET}"
  echo "${C_BG_YEL}!!  ⚠ 注意：$*${C_RESET}"
  echo "${C_BG_YEL}!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!${C_RESET}"
  echo ""
}

# 高亮「需要你手动操作」区块
print_manual_box() {
  local title="$1"
  shift
  echo ""
  echo "${C_BG_RED}████████████████████████████████████████████████████████████████${C_RESET}"
  echo "${C_BG_RED}██  ✋ 需要你手动操作（请仔细看）                              ██${C_RESET}"
  echo "${C_BG_RED}████████████████████████████████████████████████████████████████${C_RESET}"
  echo "${C_YEL}${C_BOLD}>>> $title${C_RESET}"
  echo "${C_YEL}────────────────────────────────────────────────────────────────${C_RESET}"
  while [[ $# -gt 0 ]]; do
    if [[ -z "$1" ]]; then
      echo ""
    else
      echo "${C_YEL}  ▸ ${C_RESET}$1"
    fi
    shift
  done
  echo "${C_YEL}────────────────────────────────────────────────────────────────${C_RESET}"
  echo ""
}

# 高亮「请复制以下内容」— 最显眼的一块
print_manual_json_block() {
  local settings_path="$1"
  echo ""
  echo "${C_BG_YEL}********************************************************************${C_RESET}"
  echo "${C_BG_YEL}**  ★★★  请复制以下内容（整段粘贴/合并到 settings.json）  ★★★  **${C_RESET}"
  echo "${C_BG_YEL}********************************************************************${C_RESET}"
  echo ""
  echo "${C_CYN}${C_BOLD}  【目标文件】${C_RESET}"
  echo "    ${C_BOLD}$settings_path${C_RESET}"
  echo ""
  echo "${C_CYN}${C_BOLD}  【怎么做】${C_RESET}"
  echo "    1. 用编辑器打开上面的 settings.json"
  echo "    2. 在最外层 { ... } 里 ${C_YEL}${C_BOLD}合并${C_RESET} 下面字段（不要重复同名 key）"
  echo "    3. 注意 JSON ${C_YEL}逗号${C_RESET}；${C_GRN}保留${C_RESET} 你原来的其它配置和注释"
  echo ""
  echo "${C_BG_YEL}>>>>>>>>>>  从下一行开始复制  >>>>>>>>>>${C_RESET}"
  echo "${C_DIM}# ----- COPY START（Zed settings 支持 // 注释；自动写入时会剥掉注释） -----${C_RESET}"
  cat <<EOF
  // ── httpyac 扩展相关（路径类：填绝对路径字符串）────────────────
  "httpyac": {
    // command: httpyac CLI 可执行文件路径 | 或 PATH 里的命令名 "httpyac"
    "command": "$HTTPYAC_PATH",
    // lsp_command: httpyac-lsp 路径 | 一般 ~/.local/bin/httpyac-lsp
    "lsp_command": "$LSP_PATH",
    // vtsls_command: child vtsls 路径（仅 script 引擎=vtsls 时用）| "vtsls" | which vtsls
    "vtsls_command": "$VTSLS_PATH",
    // use_builtin_script_completions: true=内置 catalog | false=只用 child vtsls（与 vtsls 互斥）
    "use_builtin_script_completions": true,
    // default_env: 文档/默认名字符串（如 "dev"|"prod"|"test"）；真正发请求以 env JSON 的 activeEnv 为准
    "default_env": "dev"
  },
  "lsp": {
    "httpyac-lsp": {
      "binary": {
        // path: Zed 启动 LSP 的绝对路径
        "path": "$LSP_PATH",
        // arguments: 传给 httpyac-lsp 的额外参数数组，通常 []
        "arguments": []
      },
      "settings": {
        // vtslsCommand: 同 httpyac.vtsls_command（LSP 优先读这里）
        "vtslsCommand": "$VTSLS_PATH",
        // vtslsEnabled: true=允许拉起 vtsls | false=强制走 builtin（即使 source 写成 vtsls）
        "vtslsEnabled": true,
        // useBuiltinScriptCompletions: true= {{}} 只用内置 catalog | false=只用 child vtsls（互斥，不能两个一起）
        // 有 vtsls 时默认 false=纯 vtsls（shadow+jsconfig+@types/node）；无 vtsls 时 true
        "useBuiltinScriptCompletions": false,
        // scriptCompletionSource: "builtin"|"catalog"|"httpyac" = 内置
        //                         "vtsls"|"ts"|"typescript" = child vtsls
        // （与 useBuiltinScriptCompletions 二选一写法，效果相同）
        "scriptCompletionSource": "vtsls"
      }
    }
  },
  "languages": {
    "HTTP": {
      // enable_language_server: true | false
      "enable_language_server": true,
      // language_servers: 只能 ["httpyac-lsp"] — 不要加 "vtsls"（整文件当 TS 会炸）
      "language_servers": ["httpyac-lsp"],
      "completions": {
        // lsp: true=用 LSP 补全 | false=关闭
        "lsp": true,
        // words: "disabled"=不要单词补全 | "fallback"=LSP 无结果时再用单词 | "enabled"=总开
        // （推荐 fallback 或 disabled，避免 Hello 盖住 Host）
        "words": "disabled"
        // 可选 words_min_length: 数字 ≥1，单词补全最小长度，常用 3
      }
    }
  }
EOF
  echo "${C_DIM}# ----- COPY END -----${C_RESET}"
  echo "${C_BG_YEL}<<<<<<<<<<  复制到上一行结束  <<<<<<<<<<${C_RESET}"
  echo ""
  echo "${C_CYN}${C_BOLD}  【配置项可选值速查】${C_RESET}"
  echo "    ${C_BOLD}script 补全引擎（{{}} 内互斥，只生效一个）${C_RESET}"
  echo "      useBuiltinScriptCompletions / use_builtin_script_completions"
  echo "        → ${C_GRN}true${C_RESET}  = 内置 catalog（@returns / .script / require 形状）  ${C_DIM}【默认】${C_RESET}"
  echo "        → ${C_YEL}false${C_RESET} = child vtsls（需 npm i -g @vtsls/language-server）"
  echo "      scriptCompletionSource"
  echo "        → ${C_GRN}\"builtin\"${C_RESET} | \"catalog\" | \"httpyac\"     = 同上内置"
  echo "        → ${C_YEL}\"vtsls\"${C_RESET}   | \"ts\" | \"typescript\" = 同上 vtsls"
  echo "      vtslsEnabled"
  echo "        → ${C_GRN}true${C_RESET}  = 允许启动 vtsls（当 source=vtsls 时）"
  echo "        → ${C_YEL}false${C_RESET} = 强制 builtin"
  echo "    ${C_BOLD}路径类（字符串）${C_RESET}"
  echo "      httpyac.command / lsp_command / vtsls_command / lsp.httpyac-lsp.binary.path"
  echo "        → 绝对路径，或 PATH 中的命令名"
  echo "    ${C_BOLD}HTTP 语言${C_RESET}"
  echo "      language_servers → 仅 ${C_GRN}[\"httpyac-lsp\"]${C_RESET}（禁止加 vtsls）"
  echo "      completions.words → ${C_GRN}\"disabled\"${C_RESET} | \"fallback\" | \"enabled\""
  echo "      completions.lsp   → true | false"
  echo "      default_env       → 任意环境名字符串（dev/prod/…）"
  echo ""
  echo "${C_MAG}${C_BOLD}  【不想手改？】${C_RESET}可强制自动写入（${C_RED}会丢掉 // 注释${C_RESET}，会先备份）："
  echo "    ${C_BOLD}FORCE_ZED_SETTINGS=1 $0${C_RESET}"
  echo ""
}

# ---------------------------------------------------------------------------
# 路径探测（不写死机器路径）
# ---------------------------------------------------------------------------
detect_os_paths() {
  local config_home="${XDG_CONFIG_HOME:-$HOME/.config}"

  if [[ "${OSTYPE:-}" == darwin* ]]; then
    IS_MACOS=1
    # 扩展：macOS 仍在 Application Support
    ZED_EXT_DIR="${HOME}/Library/Application Support/Zed/extensions/installed"
    # 设置：优先 ~/.config/zed/settings.json（当前 Zed 默认）
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
    ZED_EXT_DIR="${config_home}/zed/extensions/installed"
    if [[ ! -d "$ZED_EXT_DIR" && -d "${config_home}/zed/extensions" ]]; then
      ZED_EXT_DIR="${config_home}/zed/extensions"
    fi
    ZED_SETTINGS="${config_home}/zed/settings.json"
  fi

  LOCAL_BIN="${HOME}/.local/bin"
}

echo "=========================================="
echo " HTTPyac Client — 编译并安装到 Zed"
echo "=========================================="
echo ""

detect_os_paths
echo "【路径】"
echo "  扩展目录   → $ZED_EXT_DIR"
echo "  配置文件   → $ZED_SETTINGS"
echo "  本地 bin   → $LOCAL_BIN"
echo ""

# ---------------------------------------------------------------------------
# 1) 编译 httpyac-lsp
# ---------------------------------------------------------------------------
echo "【1/7】编译 httpyac-lsp + httpyac-run（Release）..."
if (cd lsp && cargo build --release); then
  mkdir -p target/release
  cp lsp/target/release/httpyac-lsp target/release/
  cp lsp/target/release/httpyac-run target/release/
  echo "  ✅ httpyac-lsp 编译成功"
  echo "  ✅ httpyac-run 编译成功（发送/选环境入口）"
  OK_LSP=1
else
  echo "  ❌ LSP/run 编译失败"
  exit 1
fi
echo ""

# ---------------------------------------------------------------------------
# 2) 编译 WASM 扩展（必须是 Component Model，目标 wasm32-wasip2）
#    Zed 用 component parser 加载 extension.wasm；wasip1 产物是 MVP 模块，会报：
#    "attempted to parse a wasm module with a component parser"
#    → 扩展加载失败 → httpyac-lsp 永不启动 → 只有 buffer 单词补全（Hello 等）
# ---------------------------------------------------------------------------
echo "【2/7】安装 wasm32-wasip2 目标（如需要）..."
rustup target add wasm32-wasip2 >/dev/null 2>&1 || true

echo "【3/7】编译 WASM 扩展（wasm32-wasip2 component）..."
if cargo build --release --target wasm32-wasip2; then
  :
else
  echo "  ❌ WASM 扩展编译失败"
  exit 1
fi

WASM_SRC=""
for candidate in \
  "target/wasm32-wasip2/release/zed_httpyacclient.wasm" \
  "target/wasm32-wasip2/release/httpyacclient.wasm" \
  "target/wasm32-wasip1/release/zed_httpyacclient.wasm"
do
  if [[ -f "$candidate" ]]; then
    WASM_SRC="$candidate"
    break
  fi
done

if [[ -z "$WASM_SRC" ]]; then
  echo "  ❌ 未找到 WASM 产物（期望 zed_httpyacclient.wasm）"
  ls -la target/wasm32-wasip2/release/ 2>/dev/null || true
  exit 1
fi

cp "$WASM_SRC" extension.wasm

# 校验：Zed 需要 WebAssembly Component（file 显示 version 0x1000d），不是 MVP 0x1
WASM_FILE_INFO="$(file extension.wasm 2>/dev/null || true)"
if echo "$WASM_FILE_INFO" | grep -q "0x1 (MVP)"; then
  echo "  ❌ extension.wasm 仍是 MVP 模块，不是 Component："
  echo "     $WASM_FILE_INFO"
  echo "     请使用 rustc/cargo 较新版本，并确保 --target wasm32-wasip2"
  exit 1
fi
if echo "$WASM_FILE_INFO" | grep -qE "0x1000d|component"; then
  echo "  ✅ extension.wasm 为 Component ← $WASM_SRC ($(du -h extension.wasm | awk '{print $1}'))"
else
  echo "  ⚠ 无法用 file(1) 确认 component 格式，继续安装："
  echo "     $WASM_FILE_INFO"
  echo "  ✅ extension.wasm ← $WASM_SRC ($(du -h extension.wasm | awk '{print $1}'))"
fi
OK_WASM=1
echo ""
# ---------------------------------------------------------------------------
# 3) 语法文件
# ---------------------------------------------------------------------------
echo "【4/7】检查 tree-sitter 语法..."
if [[ ! -f grammars/http.wasm ]]; then
  echo "  正在编译 grammars/http.wasm ..."
  if ! command -v tree-sitter >/dev/null 2>&1; then
    echo "  通过 npm 安装 tree-sitter-cli..."
    npm install -g tree-sitter-cli
  fi
  GRAMMAR_TMP="$(mktemp -d)"
  git clone --depth 1 https://github.com/rest-nvim/tree-sitter-http.git "$GRAMMAR_TMP/tree-sitter-http"
  (cd "$GRAMMAR_TMP/tree-sitter-http" && tree-sitter build --wasm -o http.wasm)
  mkdir -p grammars
  cp "$GRAMMAR_TMP/tree-sitter-http/http.wasm" grammars/http.wasm
  rm -rf "$GRAMMAR_TMP"
  echo "  ✅ grammars/http.wasm 已生成"
else
  echo "  ✅ grammars/http.wasm 已存在"
fi
OK_GRAMMAR=1
echo ""

# ---------------------------------------------------------------------------
# 4) 安装到 Zed 扩展目录
# ---------------------------------------------------------------------------
echo "【5/7】安装扩展文件到 Zed..."
mkdir -p "$ZED_EXT_DIR"
INSTALL_DIR="${ZED_EXT_DIR}/httpyacclient"
rm -rf "$INSTALL_DIR"
mkdir -p "$INSTALL_DIR/grammars" "$INSTALL_DIR/lsp"

cp extension.toml extension.wasm "$INSTALL_DIR/"
cp -R languages "$INSTALL_DIR/"
cp grammars/http.wasm "$INSTALL_DIR/grammars/"
if [[ -d snippets ]]; then
  cp -R snippets "$INSTALL_DIR/"
fi
cp target/release/httpyac-lsp "$INSTALL_DIR/lsp/httpyac-lsp"
cp target/release/httpyac-run "$INSTALL_DIR/lsp/httpyac-run"
chmod +x "$INSTALL_DIR/lsp/httpyac-lsp" "$INSTALL_DIR/lsp/httpyac-run"

if [[ "$IS_MACOS" -eq 1 ]]; then
  codesign --force --sign - "$INSTALL_DIR/lsp/httpyac-lsp" 2>/dev/null || true
  codesign --force --sign - "$INSTALL_DIR/lsp/httpyac-run" 2>/dev/null || true
fi

mkdir -p "$LOCAL_BIN"
cp target/release/httpyac-lsp "$LOCAL_BIN/httpyac-lsp"
cp target/release/httpyac-run "$LOCAL_BIN/httpyac-run"
chmod +x "$LOCAL_BIN/httpyac-lsp" "$LOCAL_BIN/httpyac-run"
if [[ "$IS_MACOS" -eq 1 ]]; then
  codesign --force --sign - "$LOCAL_BIN/httpyac-lsp" 2>/dev/null || true
  codesign --force --sign - "$LOCAL_BIN/httpyac-run" 2>/dev/null || true
fi

# 校验关键文件是否到位
if [[ -f "$INSTALL_DIR/extension.wasm" && -f "$INSTALL_DIR/extension.toml" \
   && -f "$INSTALL_DIR/grammars/http.wasm" && -x "$INSTALL_DIR/lsp/httpyac-lsp" \
   && -x "$LOCAL_BIN/httpyac-lsp" && -x "$LOCAL_BIN/httpyac-run" ]]; then
  echo "  ✅ 扩展已安装 → $INSTALL_DIR"
  echo "  ✅ LSP 已安装 → $LOCAL_BIN/httpyac-lsp"
  echo "  ✅ 发送入口   → $LOCAL_BIN/httpyac-run  （▶ Send / 🌐 选环境）"
  OK_EXT=1
else
  echo "  ❌ 扩展安装不完整，请检查上述路径"
  exit 1
fi
echo ""

# ---------------------------------------------------------------------------
# 5) 探测 httpyac / httpyac-lsp / vtsls
# ---------------------------------------------------------------------------
echo "【6/7】探测 httpyac / httpyac-lsp / vtsls 路径..."
HTTPYAC_PATH="$(command -v httpyac 2>/dev/null || true)"
if [[ -z "$HTTPYAC_PATH" ]]; then
  HTTPYAC_PATH="httpyac"
  OK_HTTPYAC=0
  print_manual_box "安装 httpyac CLI（否则无法发送请求）" \
    "原因：当前 PATH 中找不到 httpyac 命令" \
    "" \
    "请执行：" \
    "  npm install -g httpyac" \
    "" \
    "然后验证：" \
    "  which httpyac && httpyac --version"
  WARNINGS+=("未找到 httpyac CLI")
  MANUAL_REQUIRED+=("安装 httpyac：npm install -g httpyac")
else
  echo "  ✅ httpyac     = $HTTPYAC_PATH"
  OK_HTTPYAC=1
fi

LSP_PATH="$LOCAL_BIN/httpyac-lsp"
if [[ -x "$LOCAL_BIN/httpyac-lsp" ]]; then
  LSP_PATH="$LOCAL_BIN/httpyac-lsp"
elif command -v httpyac-lsp >/dev/null 2>&1; then
  LSP_PATH="$(command -v httpyac-lsp)"
fi
echo "  ✅ httpyac-lsp = $LSP_PATH"
if [[ -x "$LOCAL_BIN/httpyac-run" ]]; then
  echo "  ✅ httpyac-run = $LOCAL_BIN/httpyac-run"
else
  echo "  ⚠️  httpyac-run 未安装"
  WARNINGS+=("httpyac-run 缺失，Send 按钮可能失败")
fi

# vtsls = @vtsls/language-server（script 区内完整 TS/Node IntelliSense）
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
  print_manual_box "安装 vtsls（仅当 script 引擎选 vtsls 时需要）" \
    "原因：当前 PATH 中找不到 vtsls（@vtsls/language-server）" \
    "说明：默认 useBuiltinScriptCompletions=true 用内置 catalog，可不装 vtsls" \
    "      若要用 child vtsls：useBuiltinScriptCompletions=false 或 scriptCompletionSource=\"vtsls\"" \
    "" \
    "请执行：" \
    "  npm install -g @vtsls/language-server" \
    "" \
    "然后验证：" \
    "  which vtsls && vtsls --version" \
    "" \
    "装好后写入 settings（注释=可选值）：" \
    "  \"vtslsCommand\": \"\$(which vtsls)\"   // 路径字符串" \
    "  \"useBuiltinScriptCompletions\": false // true=内置 | false=vtsls（互斥）" \
    "  \"scriptCompletionSource\": \"vtsls\"  // \"builtin\"|\"catalog\"|\"httpyac\" | \"vtsls\"|\"ts\"" \
    "  \"vtslsEnabled\": true                 // true=允许 vtsls | false=强制 builtin"
  WARNINGS+=("未找到 vtsls — 保持 useBuiltinScriptCompletions=true 即可用内置 catalog")
  MANUAL_OPTIONAL+=("可选：npm i -g @vtsls/language-server，再设 useBuiltinScriptCompletions=false 用 vtsls")
else
  echo "  ✅ vtsls      = $VTSLS_PATH"
  OK_VTSLS=1
fi
echo ""

case ":${PATH}:" in
  *":${LOCAL_BIN}:"*) ;;
  *)
    print_manual_box "把 ~/.local/bin 加入 PATH（推荐 / 发请求需要）" \
      "原因：$LOCAL_BIN 不在当前 PATH 中" \
      "说明：Zed 的 ▶ Send 会调用 httpyac-run，必须在 PATH 里能找到" \
      "" \
      "请在 ~/.zshrc 或 ~/.bashrc 中加入一行：" \
      "  export PATH=\"\$HOME/.local/bin:\$PATH\"" \
      "" \
      "然后执行： source ~/.zshrc   （或重新开终端）" \
      "再完全退出并重启 Zed"
    WARNINGS+=("\$HOME/.local/bin 不在 PATH — Send 可能找不到 httpyac-run")
    MANUAL_REQUIRED+=("PATH 增加 \$HOME/.local/bin 并重启 Zed（否则无环境按钮/发请求失败）")
    ;;
esac

# ---------------------------------------------------------------------------
# 6) settings.json（有注释则默认不覆盖）
# ---------------------------------------------------------------------------
echo "【7/7】处理 Zed settings.json..."
if [[ "${SKIP_ZED_SETTINGS:-0}" == "1" ]]; then
  echo "  ⏭️  已设置 SKIP_ZED_SETTINGS=1，未修改配置文件"
  OK_SETTINGS="已跳过（你指定了跳过）"
  MANUAL_OPTIONAL+=("若需要可自行编辑 settings：$ZED_SETTINGS")
elif [[ "${FORCE_ZED_SETTINGS:-0}" != "1" ]] && [[ -f "$ZED_SETTINGS" ]] && \
     grep -qE '^\s*//|/\*' "$ZED_SETTINGS" 2>/dev/null; then
  print_manual_box "合并 Zed settings.json（本次未自动写入）" \
    "原因：配置文件含 // 注释，自动写入会丢掉注释，故跳过" \
    "文件：$ZED_SETTINGS" \
    "" \
    "说明：扩展本体已装好，不配 settings 通常也能用；" \
    "      写入配置可固定 httpyac / LSP 绝对路径，更稳妥。" \
    "" \
    "▼ 请打开该文件，把下方 JSON 片段合并进顶层 { } 中"
  print_manual_json_block "$ZED_SETTINGS"
  OK_SETTINGS="未写入（有注释 → 需你手动合并）"
  WARNINGS+=("settings 未自动写入")
  MANUAL_OPTIONAL+=("手动合并 settings.json（见上方「需要你手动操作」框）")
else
  mkdir -p "$(dirname "$ZED_SETTINGS")"
  if command -v python3 >/dev/null 2>&1; then
    SET_OUT="$(
    HTTPYAC_PATH="$HTTPYAC_PATH" LSP_PATH="$LSP_PATH" VTSLS_PATH="$VTSLS_PATH" \
      FORCE_ZED_SETTINGS="${FORCE_ZED_SETTINGS:-0}" \
      python3 - "$ZED_SETTINGS" <<'PY'
import json, os, re, shutil, sys, time
from copy import deepcopy

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

before = deepcopy(settings)
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
setp(["lsp", "httpyac-lsp", "settings", "vtslsEnabled"], True, force_update=False)
# Script tips: builtin XOR vtsls — pure vtsls when binary found (shadow+jsconfig+@types/node)
_use_vtsls = (ok_vtsls == 1)
setp(["lsp", "httpyac-lsp", "settings", "useBuiltinScriptCompletions"], (not _use_vtsls), force_update=True)
setp(["lsp", "httpyac-lsp", "settings", "scriptCompletionSource"], ("vtsls" if _use_vtsls else "builtin"), force_update=True)
setp(["httpyac", "use_builtin_script_completions"], True, force_update=False)

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
        print_manual_box "合并 Zed settings.json（检测到注释，未自动写入）" \
          "文件：$ZED_SETTINGS" \
          "▼ 请手动合并下方 JSON 片段"
        print_manual_json_block "$ZED_SETTINGS"
        OK_SETTINGS="未写入（有注释 → 需你手动合并）"
        WARNINGS+=("settings 未自动写入")
        MANUAL_OPTIONAL+=("手动合并 settings.json（见上方框）")
        ;;
      *PARSE_ERROR*)
        print_manual_box "修复 settings.json 后重试" \
          "原因：无法解析 $ZED_SETTINGS" \
          "请检查 JSON/JSONC 语法，或手动加入 httpyac 相关配置"
        print_manual_json_block "$ZED_SETTINGS"
        OK_SETTINGS="未写入（解析失败 → 需你手动处理）"
        WARNINGS+=("settings 解析失败")
        MANUAL_OPTIONAL+=("修复并合并 settings.json")
        ;;
      *UP_TO_DATE*)
        echo "  ✅ 配置已是最新，无需写入（无需你操作）"
        OK_SETTINGS="已就绪（无需改动）"
        ;;
      *MERGED:*)
        if [[ "$SET_OUT" == *BACKUP:* ]]; then
          echo "  📦 已备份：$(echo "$SET_OUT" | tr ' ' '\n' | grep '^BACKUP:' | head -1 | cut -d: -f2-)"
        fi
        echo "  ✅ 已合并写入 settings.json（无重复 key，无需你再改）"
        OK_SETTINGS="已写入（无需你操作）"
        if [[ "$SET_OUT" == *FORCE_NO_COMMENTS* ]]; then
          echo "  ⚠️  强制覆盖：原文件中的 // 注释已丢失"
        fi
        ;;
      *)
        echo "  $SET_OUT"
        OK_SETTINGS="已处理"
        ;;
    esac
  else
    print_manual_box "安装 python3 或手动改 settings" \
      "原因：未找到 python3，无法自动合并配置" \
      "文件：$ZED_SETTINGS"
    print_manual_json_block "$ZED_SETTINGS"
    OK_SETTINGS="未写入（无 python3 → 需你手动合并）"
    WARNINGS+=("无 python3，settings 未改")
    MANUAL_OPTIONAL+=("手动合并 settings.json 或安装 python3 后重跑")
  fi
fi

# ---------------------------------------------------------------------------
# 最终结论
# ---------------------------------------------------------------------------
echo ""
echo "=========================================="
echo "           安装结果结论"
echo "=========================================="
echo ""

CORE_OK=1
[[ "$OK_LSP" -eq 1 ]]     || CORE_OK=0
[[ "$OK_WASM" -eq 1 ]]    || CORE_OK=0
[[ "$OK_GRAMMAR" -eq 1 ]] || CORE_OK=0
[[ "$OK_EXT" -eq 1 ]]     || CORE_OK=0

if [[ "$CORE_OK" -eq 1 ]]; then
  echo "【结论】✅ 扩展安装成功"
else
  echo "【结论】❌ 扩展安装失败（核心步骤有错误）"
fi
echo ""
echo "  检查项："
echo "  ├─ httpyac-lsp 编译     $([ "$OK_LSP" -eq 1 ] && echo '✅ 成功' || echo '❌ 失败')"
echo "  ├─ WASM 扩展编译       $([ "$OK_WASM" -eq 1 ] && echo '✅ 成功' || echo '❌ 失败')"
echo "  ├─ 语法文件            $([ "$OK_GRAMMAR" -eq 1 ] && echo '✅ 成功' || echo '❌ 失败')"
echo "  ├─ 安装到 Zed 扩展目录 $([ "$OK_EXT" -eq 1 ] && echo '✅ 成功' || echo '❌ 失败')"
echo "  ├─ httpyac CLI         $([ "$OK_HTTPYAC" -eq 1 ] && echo '✅ 已找到' || echo '⚠️  未找到（发请求前需安装）')"
echo "  ├─ vtsls (script TS)   $([ "${OK_VTSLS:-0}" -eq 1 ] && echo '✅ 已找到' || echo '⚠️  未找到（script 完整补全需安装）')"
echo "  └─ settings.json       $OK_SETTINGS"
echo ""
echo "  安装位置："
echo "  ├─ 扩展：$INSTALL_DIR"
echo "  ├─ LSP ：$LSP_PATH"
echo "  ├─ httpyac：$HTTPYAC_PATH"
echo "  └─ vtsls：$VTSLS_PATH"
echo ""

if [[ ${#WARNINGS[@]} -gt 0 ]]; then
  echo "  注意事项："
  for w in "${WARNINGS[@]}"; do
    echo "  ⚠️  $w"
  done
  echo ""
fi

# —— 需要你动手的清单（重点高亮）——
echo ""
echo "${C_BG_RED}████████████████████████████████████████████████████████████████${C_RESET}"
echo "${C_BG_RED}██  ✋ 需要你手动完成的事项（请逐项打勾）                        ██${C_RESET}"
echo "${C_BG_RED}████████████████████████████████████████████████████████████████${C_RESET}"
echo ""
HAS_MANUAL=0
if [[ ${#MANUAL_REQUIRED[@]} -gt 0 ]]; then
  HAS_MANUAL=1
  echo "${C_BG_YEL}  【必做】不处理可能影响发请求 / 使用  ${C_RESET}"
  local_i=1
  for item in "${MANUAL_REQUIRED[@]}"; do
    echo "    ${C_RED}${C_BOLD}$local_i.${C_RESET} ${C_BOLD}$item${C_RESET}"
    local_i=$((local_i + 1))
  done
  echo ""
fi
if [[ ${#MANUAL_OPTIONAL[@]} -gt 0 ]]; then
  HAS_MANUAL=1
  echo "${C_BG_CYN}  【选做】不处理一般也能用，建议做  ${C_RESET}"
  local_i=1
  for item in "${MANUAL_OPTIONAL[@]}"; do
    echo "    ${C_CYN}$local_i.${C_RESET} $item"
    local_i=$((local_i + 1))
  done
  echo ""
fi
# 重启 Zed 始终是必做（装完扩展必须）
if [[ "$CORE_OK" -eq 1 ]]; then
  HAS_MANUAL=1
  echo "${C_BG_YEL}  【必做】安装完成后请立刻  ${C_RESET}"
  echo "    ${C_RED}${C_BOLD}1.${C_RESET} ${C_BOLD}完全退出 Zed（macOS: Cmd+Q，不要只关窗口）再重新打开${C_RESET}"
  echo "    ${C_RED}${C_BOLD}2.${C_RESET} ${C_BOLD}打开 examples/basic.http → 点请求左侧 ▶ Send 试发${C_RESET}"
  if [[ "$OK_HTTPYAC" -eq 1 ]]; then
    echo "    ${C_DIM}3.${C_RESET} （可选）examples/environments.http → 点 🌐 Env:* 切换环境"
  fi
  echo ""
fi
if [[ "$HAS_MANUAL" -eq 0 ]]; then
  echo "  （当前没有额外手动项）"
  echo ""
fi

if [[ "$CORE_OK" -eq 1 ]]; then
  echo "${C_BG_YEL}!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!${C_RESET}"
  echo "${C_GRN}${C_BOLD}【结论】✅ 扩展本体安装成功${C_RESET}"
  echo "         请完成上面 ${C_YEL}${C_BOLD}黄底/红底${C_RESET} 标出的「必做」项后再使用。"
  echo "${C_BG_YEL}!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!${C_RESET}"
  echo ""
  exit 0
else
  echo "${C_BG_RED}!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!${C_RESET}"
  echo "${C_RED}${C_BOLD}【结论】❌ 安装未完成${C_RESET}"
  echo "         请根据上方 ❌ 与「需要你手动操作」排查后重试。"
  echo "${C_BG_RED}!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!${C_RESET}"
  echo ""
  exit 1
fi
