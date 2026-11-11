#!/usr/bin/env node
/**
 * 把版本号同步写入所有需要的地方（CI 里由 git tag 驱动）。
 *
 * 用法：
 *   node scripts/set-version.mjs 1.2.1
 *
 * 会修改：
 *   - package.json（前端）
 *   - src-tauri/tauri.conf.json（Tauri 打包版本 → 决定 exe/msi/dmg 的版本号）
 *   - src-tauri/Cargo.toml（Rust 包版本）
 *   - src-tauri/Cargo.lock（同步锁定文件里的 tomatossh 条目）
 */
import { readFileSync, writeFileSync } from "node:fs";

const raw = process.argv[2];
if (!raw) {
  console.error("用法: node scripts/set-version.mjs <version>  例如 1.2.1");
  process.exit(1);
}
const version = raw.trim().replace(/^v/i, "");
if (!/^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/.test(version)) {
  console.error(`版本号不是合法的 semver: ${raw}`);
  process.exit(1);
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
let cargoDone = false;
cargo = cargo.replace(/^version\s*=\s*"[^"]*"/m, (m) => {
  if (cargoDone) return m;
  cargoDone = true;
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
  (_m, prefix) => `${prefix}"${version}"`,
);
if (before === lock) {
  console.warn("警告: Cargo.lock 中未找到 tomatossh 条目，跳过");
} else {
  writeFileSync(lockPath, lock, "utf8");
  console.log(`${lockPath} -> ${version}`);
}

console.log(`✅ 版本号统一为 ${version}`);
