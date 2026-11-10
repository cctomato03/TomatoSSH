#!/usr/bin/env python3
"""前后端 IPC 契约静态校验。

检查三件事：
1. 前端 `invoke("cmd", {…})` 里用到的命令，Rust 侧是否真的有 `#[tauri::command]`；
2. 前端传的参数名（camelCase）能否对上 Rust 形参（snake_case）；
3. 每个命令是否都在 `src-tauri/src/lib.rs` 的 `generate_handler!` 里注册。

用法：
    python scripts/check_contract.py
"""
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
api_ts = (ROOT / "src/lib/api.ts").read_text(encoding="utf-8")
commands_rs = (ROOT / "src-tauri/src/commands.rs").read_text(encoding="utf-8")
lib_rs = (ROOT / "src-tauri/src/lib.rs").read_text(encoding="utf-8")


def camel(snake: str) -> str:
    parts = snake.split("_")
    return parts[0] + "".join(p[:1].upper() + p[1:] for p in parts[1:])


def frontend_calls() -> dict[str, list[str]]:
    calls: dict[str, list[str]] = {}
    pattern = r'call<[^>]*>\(\s*"([a-z_]+)"\s*(?:,\s*\{([^}]*)\})?'
    for m in re.finditer(pattern, api_ts):
        name, args = m.group(1), m.group(2) or ""
        keys = [a.split(":")[0].strip() for a in args.split(",") if a.strip()]
        calls[name] = keys
    return calls


def rust_commands() -> dict[str, list[str]]:
    out: dict[str, list[str]] = {}
    pattern = r"#\[tauri::command\]\s*\npub (?:async )?fn (\w+)\(([^)]*)\)"
    for m in re.finditer(pattern, commands_rs, re.S):
        name, params = m.group(1), m.group(2)
        names = []
        for p in params.split(","):
            p = p.strip()
            if not p or "State<" in p or p.startswith("app:"):
                continue
            names.append(p.split(":")[0].strip())
        out[name] = names
    return out


def main() -> int:
    calls = frontend_calls()
    rust = rust_commands()
    registered = set(re.findall(r"commands::(\w+)", lib_rs))

    problems: list[str] = []
    for name, keys in calls.items():
        if name not in rust:
            problems.append(f"前端调用了 Rust 中不存在的命令: {name}")
            continue
        expected = [camel(p) for p in rust[name]]
        missing = [k for k in keys if k not in expected]
        if missing:
            problems.append(f"{name}: 前端参数 {missing} 与 Rust 形参 {expected} 不匹配")
        if name not in registered:
            problems.append(f"{name}: 未在 lib.rs 的 generate_handler! 中注册")

    for name in rust:
        if name not in calls and name not in {"vault_load"}:
            problems.append(f"Rust 命令 {name} 未被前端调用")

    print(f"前端 invoke 命令 {len(calls)} 个 / Rust 命令 {len(rust)} 个 / 已注册 {len(registered & set(rust))} 个")
    if problems:
        print("\n发现问题:")
        for p in problems:
            print("  -", p)
        return 1
    print("OK: 命令名、参数名（camelCase <-> snake_case）、注册表完全一致")
    return 0


if __name__ == "__main__":
    sys.exit(main())
