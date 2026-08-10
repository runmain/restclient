//! httpyac-run — Zed gutter / task entry.
//!
//! Usage:
//!   httpyac-run <file.http> <line>                 # Send（用当前选中环境）
//!   httpyac-run <file.http> <line> --env prod      # Send 指定环境
//!   httpyac-run <file.http> <line> --pick-env      # 选环境并发送
//!   httpyac-run <file.http> --switch-env           # 只切换环境，不发送
//!   httpyac-run <file.http> <line> --switch-env    # 同上（行号可忽略）
//!
//! 当前环境：仅使用 http-client.env.json 的 activeEnv（切换时只改该字段，不另建文件）

use std::io::{self, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

use httpyac_lsp::{
    build_httpyac_send_args, file_dir, resolve_effective_env_with_errors, resolve_httpyac_bin,
    save_active_env,
};

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        print_usage();
        std::process::exit(2);
    }

    let file = args.remove(0);

    // Parse flags first so --switch-env works without a line number
    let mut env_name: Option<String> = None;
    let mut pick = false;
    let mut switch_only = false;
    let mut positional: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--env" | "-e" => {
                if i + 1 < args.len() {
                    env_name = Some(args[i + 1].clone());
                    i += 2;
                } else {
                    eprintln!("错误: --env 需要环境名");
                    std::process::exit(2);
                }
            }
            "--pick-env" => {
                pick = true;
                i += 1;
            }
            "--switch-env" => {
                switch_only = true;
                pick = true; // always show menu when switching
                i += 1;
            }
            other if other.starts_with('-') => {
                eprintln!("未知参数: {other}");
                print_usage();
                std::process::exit(2);
            }
            other => {
                positional.push(other.to_string());
                i += 1;
            }
        }
    }

    let line = positional.first().cloned();

    if !switch_only && line.is_none() {
        eprintln!("错误: 发送请求需要行号");
        print_usage();
        std::process::exit(2);
    }

    let path = PathBuf::from(&file);
    let abs_path = std::fs::canonicalize(&path).unwrap_or_else(|_| {
        if path.is_absolute() {
            path.clone()
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(&path)
        }
    });
    let dir = file_dir(&abs_path);
    let (effective, names, env_errors) = resolve_effective_env_with_errors(&dir);
    for error in env_errors {
        eprintln!("环境文件警告: {error}");
    }

    if !switch_only && !pick && env_name.is_none() {
        if let Some(active) = effective.as_deref() {
            if !names.iter().any(|name| name == active) {
                eprintln!("activeEnv `{active}` 不存在于可用环境: {names:?}");
                std::process::exit(1);
            }
        }
    }

    // ── 只切换环境，不发送 ──
    if switch_only {
        if names.is_empty() {
            eprintln!(
                "未找到环境。请在 .http 同目录或上级放置 http-client.env.json\n\
                 搜索起点: {}",
                dir.display()
            );
            std::process::exit(1);
        }
        let chosen = if let Some(e) = env_name {
            if !names.contains(&e) {
                eprintln!("环境 `{e}` 不在列表中: {names:?}");
                std::process::exit(1);
            }
            e
        } else {
            pick_environment(&names, effective.as_deref())
        };

        match save_active_env(&dir, &chosen) {
            Ok(()) => {
                eprintln!();
                eprintln!("╔════════════════════════════════════════╗");
                eprintln!("║  ✅ 已切换环境（未发送请求）           ║");
                eprintln!("╠════════════════════════════════════════╣");
                eprintln!("║  当前环境: {chosen}");
                eprintln!("║  仅更新 http-client.env.json 的 activeEnv");
                eprintln!("║  （不创建任何额外文件）");
                eprintln!("╚════════════════════════════════════════╝");
                eprintln!();
                eprintln!("下次点 ▶ Send Request 将使用环境: {chosen}");
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("保存环境失败: {e}");
                std::process::exit(1);
            }
        }
    }

    // ── 发送请求 ──
    let line = line.expect("line required for send");

    if pick {
        if names.is_empty() {
            eprintln!(
                "未找到环境文件。请在 .http 同目录或上级目录放置 http-client.env.json\n\
                 当前搜索起点: {}",
                dir.display()
            );
            std::process::exit(1);
        }
        let chosen = pick_environment(&names, effective.as_deref());
        // 选环境并发送时，也持久化，方便下次 Send
        if let Err(e) = save_active_env(&dir, &chosen) {
            eprintln!("警告: 未能保存当前环境: {e}");
        }
        env_name = Some(chosen);
    } else if env_name.is_none() {
        env_name = effective.clone();
    }

    let httpyac = resolve_httpyac_bin();

    // If URL has no scheme: force http:// (not https — may be bare IP).
    // With Host header + path-only URL → http://{Host}{path}
    let original = match std::fs::read_to_string(&abs_path) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("读取 HTTP 文件失败 ({}): {e}", abs_path.display());
            std::process::exit(1);
        }
    };
    let line_num: usize = line.parse().unwrap_or(1);
    let (send_content, url_note) =
        match httpyac_lsp::rewrite_url_scheme_in_content(&original, line_num) {
            Ok(result) => result,
            Err(e) => {
                eprintln!("解析请求行失败（未展开变量，变量仍交给 httpyac）: {e}");
                std::process::exit(1);
            }
        };

    let use_temp = url_note.is_some() && send_content != original;
    let (file_for_cli, tmp_path) = if use_temp {
        let tmp = dir.join(format!(".httpyac-run-tmp-{}.http", std::process::id()));
        if let Err(e) = std::fs::write(&tmp, &send_content) {
            eprintln!("警告: 无法写临时文件补全 http:// ({e})，使用原文件");
            (
                abs_path
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| abs_path.to_string_lossy().to_string()),
                None,
            )
        } else {
            (
                tmp.file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| tmp.to_string_lossy().to_string()),
                Some(tmp),
            )
        }
    } else {
        (
            abs_path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| abs_path.to_string_lossy().to_string()),
            None,
        )
    };

    let send_args = build_httpyac_send_args(&file_for_cli, &line, env_name.as_deref());

    eprintln!("────────────────────────────────────────");
    eprintln!("  引擎: {httpyac}");
    eprintln!("  目录: {}", dir.display());
    eprintln!("  文件: {file_for_cli}");
    eprintln!("  行号: {line}");
    if let Some(ref e) = env_name {
        eprintln!("  环境: {e}");
    } else {
        eprintln!("  环境: (无)");
    }
    if let Some(ref n) = url_note {
        eprintln!("  URL:  {n}");
    }
    eprintln!("  命令: {httpyac} {}", send_args.join(" "));
    eprintln!("────────────────────────────────────────");
    eprintln!();

    let status = Command::new(&httpyac)
        .args(&send_args)
        .current_dir(&dir)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status();

    if let Some(tmp) = tmp_path {
        let _ = std::fs::remove_file(&tmp);
    }

    match status {
        Ok(s) => {
            let code = s.code().unwrap_or(1);
            if !s.success() {
                eprintln!("httpyac 执行失败，退出码: {code}");
            }
            std::process::exit(code);
        }
        Err(e) => {
            eprintln!(
                "无法启动 httpyac ({httpyac}): {e}\n\
                 请安装: npm install -g httpyac\n\
                 或设置: export HTTPYAC_BIN=$(which httpyac)"
            );
            std::process::exit(127);
        }
    }
}

fn print_usage() {
    eprintln!(
        "用法:\n\
         \n\
         \thttpyac-run <file.http> <行号>                 发送（当前环境）\n\
         \thttpyac-run <file.http> <行号> --env prod      发送并指定环境\n\
         \thttpyac-run <file.http> <行号> --pick-env      选环境并发送\n\
         \thttpyac-run <file.http> --switch-env           只切换环境，不发送\n\
         \n\
         当前环境: 仅 http-client.env.json 的 activeEnv 字段"
    );
}

fn pick_environment(names: &[String], active: Option<&str>) -> String {
    eprintln!();
    eprintln!("╔══════════════════════════════════════╗");
    eprintln!("║  选择环境 (http-client.env.json)     ║");
    eprintln!("╠══════════════════════════════════════╣");
    for (idx, name) in names.iter().enumerate() {
        let mark = if Some(name.as_str()) == active {
            " ← 当前"
        } else {
            ""
        };
        eprintln!("║  [{}] {}{}", idx + 1, name, mark);
    }
    eprintln!("╚══════════════════════════════════════╝");
    eprint!("请输入编号 (回车保持当前/默认 1): ");
    let _ = io::stderr().flush();

    let mut input = String::new();
    if io::stdin().read_line(&mut input).is_err() {
        return active
            .map(|s| s.to_string())
            .unwrap_or_else(|| names[0].clone());
    }
    let input = input.trim();
    if input.is_empty() {
        return active
            .map(|s| s.to_string())
            .unwrap_or_else(|| names[0].clone());
    }
    if let Ok(n) = input.parse::<usize>() {
        if n >= 1 && n <= names.len() {
            return names[n - 1].clone();
        }
    }
    if names.iter().any(|n| n == input) {
        return input.to_string();
    }
    eprintln!("无效选择，使用: {}", names[0]);
    names[0].clone()
}
