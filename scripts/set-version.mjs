#!/usr/bin/env node
/**
 * 把版本号同步写入所有需要的地方（CI 里由 git tag 驱动）。
 *
 * 用法：
 *   node scripts/set-version.mjs 1.2.1              # 普通版本
 *   node scripts/set-version.mjs v1.0.0-rc          # 预发布：默认只取数字部分 1.0.0
 *   node scripts/set-version.mjs 1.0.0-rc --keep-pre # 保留 -rc（仅 NSIS/AppImage/dmg 可用）
 *
 * 为什么要这样：Windows 的 MSI(WiX) 要求预发布标识必须是纯数字且 ≤65535，
 * 像 1.0.0-rc 这种会直接报
 *   optional pre-release identifier in app version must be numeric-only ...
 * 编码阶段直接用 1.0.0、把 RC 体现在 git tag / Release 名上，可以四平台都出包。
 *
 * 会修改：package.json、src-tauri/tauri.conf.json、src-tauri/Cargo.toml、src-tauri/Cargo.lock
 */
import { readFileSync, writeFileSync } from "node:fs";

const args = process.argv.slice(2);
const keepPre = args.includes("--keep-pre");
const raw = args.find((a) => !a.startsWith("-"));
if (!raw) {
  console.error("用法: node scripts/set-version.mjs <version> [--keep-pre]");
  process.exit(1);
}

// 允许 1.2 这种两位写法（tag 里常见），自动补成 1.2.0
let input = raw.trim().replace(/^v/i, "");
if (/^\d+\.\d+$/.test(input)) input = `${input}.0`;
const m = /^(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?(?:\+([0-9A-Za-z.-]+))?$/.exec(input);
if (!m) {
  console.error(`版本号不是合法的 semver: ${raw}`);
  process.exit(1);
}
const [, major, minor, patch, pre] = m;

let version = input;
if (pre !== undefined && !keepPre && !/^\d+$/.test(pre)) {
  version = `${major}.${minor}.${patch}`;
  console.warn(
    `提示: 预发布标识 "${pre}" 不是纯数字，Windows MSI 无法打包，已改用 ${version}。` +
      `\n      RC 语义请通过 git tag / Release 名体现（例如 tag v${major}.${minor}.${patch}-${pre}）。` +
      `\n      若只想出 NSIS/AppImage/dmg，可加 --keep-pre 保留完整版本号。`,
  );
}

function patchJson(path, mutate) {
  const text = readFileSync(path, "utf8");
  const data = JSON.parse(text);
  mutate(data);
  writeFileSync(path, JSON.stringify(data, null, 2) + "\n", "utf8");
  console.log(`${path} -> ${version}`);
}

patchJson("package.json", (d) => {
  d.version = version;
});
patchJson("src-tauri/tauri.conf.json", (d) => {
  d.version = version;
});

// Cargo.toml：只改第一个 version = "..."（即 [package] 下的）
const cargoPath = "src-tauri/Cargo.toml";
let cargo = readFileSync(cargoPath, "utf8");
let done = false;
cargo = cargo.replace(/^version\s*=\s*"[^"]*"/m, (line) => {
  if (done) return line;
  done = true;
  return `version = "${version}"`;
});
writeFileSync(cargoPath, cargo, "utf8");
console.log(`${cargoPath} -> ${version}`);

// Cargo.lock：同步 [[package]] name = "tomatossh" 的版本
const lockPath = "src-tauri/Cargo.lock";
let lock = readFileSync(lockPath, "utf8");
const before = lock;
lock = lock.replace(
  /(name = "tomatossh"\r?\nversion = )"[^"]*"/,
  (_all, prefix) => `${prefix}"${version}"`,
);
if (before === lock) {
  console.warn("警告: Cargo.lock 中未找到 tomatossh 条目，跳过");
} else {
  writeFileSync(lockPath, lock, "utf8");
  console.log(`${lockPath} -> ${version}`);
}

console.log(`✅ 版本号统一为 ${version}`);
