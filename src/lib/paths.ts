/** 跨平台路径小工具（同时处理 Windows 与 POSIX 风格） */

export function isWindowsPath(p: string): boolean {
  return /^[a-zA-Z]:[\\/]/.test(p) || p.startsWith("\\\\");
}

export function joinPath(dir: string, name: string): string {
  if (!dir) return name;
  if (isWindowsPath(dir)) {
    return dir.endsWith("\\") || dir.endsWith("/") ? `${dir}${name}` : `${dir}\\${name}`;
  }
  return dir.endsWith("/") ? `${dir}${name}` : `${dir}/${name}`;
}

export function parentPath(p: string): string {
  if (!p) return p;
  const win = isWindowsPath(p);
  const sep = win ? "\\" : "/";
  let s = p.replace(/[\\/]+$/, "");
  if (win) {
    if (/^[a-zA-Z]:$/.test(s)) return `${s}\\`;
  }
  if (s === "" ) return sep;
  const idx = Math.max(s.lastIndexOf("/"), win ? s.lastIndexOf("\\") : -1);
  if (idx < 0) return win ? `${s}\\` : "/";
  if (idx === 0) return "/";
  if (win && idx === 2 && /^[a-zA-Z]:/.test(s)) return s.slice(0, 3);
  return s.slice(0, idx);
}

export function baseName(p: string): string {
  const s = p.replace(/[\\/]+$/, "");
  const idx = Math.max(s.lastIndexOf("/"), s.lastIndexOf("\\"));
  return idx >= 0 ? s.slice(idx + 1) : s;
}

export function formatSize(bytes: number): string {
  if (bytes < 0) return "-";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let v = bytes;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return i === 0 ? `${v} ${units[0]}` : `${v.toFixed(v >= 10 ? 0 : 1)} ${units[i]}`;
}

export function formatTime(secs: number): string {
  if (!secs) return "-";
  const d = new Date(secs * 1000);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}
