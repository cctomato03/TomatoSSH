import { useEffect, useState } from "react";
import { Palette, RotateCcw, Type } from "lucide-react";
import { open as openDirDialog } from "@tauri-apps/plugin-dialog";
import { api } from "../lib/api";
import { THEMES, getTheme } from "../lib/themes";
import { DEFAULT_SETTINGS, useVault } from "../store/useVault";
import { useUi } from "../store/useUi";
import { Modal } from "./Modal";

const FONT_PRESETS = [
  "JetBrains Mono, Fira Code, Cascadia Code, Menlo, Consolas, monospace",
  "Cascadia Code, Consolas, monospace",
  "Consolas, Courier New, monospace",
  "Menlo, Monaco, Courier New, monospace",
  "Ubuntu Mono, DejaVu Sans Mono, monospace",
  "Source Code Pro, Consolas, monospace",
  "monospace",
];

export function SettingsDialog() {
  const open = useUi((s) => s.settingsOpen);
  const setOpen = useUi((s) => s.setSettingsOpen);
  const settings = useVault((s) => s.settings);
  const update = useVault((s) => s.updateSettings);
  const [version, setVersion] = useState("");

  useEffect(() => {
    if (!open) return;
    api
      .appVersion()
      .then(setVersion)
      .catch(() => undefined);
  }, [open]);

  if (!open) return null;
  const close = () => setOpen(false);
  const theme = getTheme(settings.theme);

  return (
    <Modal
      title="设置"
      onClose={close}
      wide
      footer={
        <>
          <button
            className="btn ghost"
            onClick={() => update({ ...DEFAULT_SETTINGS, sync: settings.sync })}
          >
            <RotateCcw size={13} /> 恢复默认外观
          </button>
          <button className="btn primary" onClick={close}>
            完成
          </button>
        </>
      }
    >
      <div className="field">
        <label>
          <Palette size={11} /> 主题（{THEMES.length} 套内置配色）
        </label>
        <div className="theme-grid">
          {THEMES.map((t) => (
            <button
              key={t.id}
              className={`theme-card${t.id === theme.id ? " on" : ""}`}
              onClick={() => update({ theme: t.id })}
            >
              <span className="theme-name">{t.name}</span>
              <span className="theme-swatches">
                <i style={{ background: t.ui.bg }} />
                <i style={{ background: t.ui.panel }} />
                <i style={{ background: t.ui.accent }} />
                <i style={{ background: t.term.green }} />
                <i style={{ background: t.term.blue ?? t.ui.accent }} />
                <i style={{ background: t.term.yellow }} />
              </span>
            </button>
          ))}
        </div>
      </div>

      <div className="field">
        <label>
          <Type size={11} /> 字体
        </label>
        <select value={settings.fontFamily} onChange={(e) => update({ fontFamily: e.target.value })}>
          {FONT_PRESETS.map((f) => (
            <option key={f} value={f}>
              {f.split(",")[0]}
            </option>
          ))}
        </select>
      </div>

      <div className="field">
        <label>字号</label>
        <div className="range-row">
          <input
            type="range"
            min={10}
            max={28}
            step={1}
            value={settings.fontSize}
            onChange={(e) => update({ fontSize: Number(e.target.value) })}
          />
          <span className="range-val">{settings.fontSize}px</span>
        </div>
      </div>

      <div className="field">
        <label>行高</label>
        <div className="range-row">
          <input
            type="range"
            min={1}
            max={2}
            step={0.05}
            value={settings.lineHeight}
            onChange={(e) => update({ lineHeight: Number(e.target.value) })}
          />
          <span className="range-val">{settings.lineHeight.toFixed(2)}</span>
        </div>
      </div>

      <div className="field">
        <label>回滚缓冲（行）</label>
        <div className="range-row">
          <input
            type="range"
            min={1000}
            max={50000}
            step={1000}
            value={settings.scrollback}
            onChange={(e) => update({ scrollback: Number(e.target.value) })}
          />
          <span className="range-val">{settings.scrollback}</span>
        </div>
      </div>

      <div className="field">
        <label>串口日志目录（按天分文件：<code>&lt;端口&gt;-YYYY-MM-DD.log</code>）</label>
        <div style={{ display: "flex", gap: 6 }}>
          <input
            value={settings.serialLogDir}
            readOnly
            placeholder="未设置（首次点「录制」时会让你选目录）"
            style={{ flex: 1, minWidth: 0 }}
          />
          <button
            className="btn"
            onClick={async () => {
              const picked = await openDirDialog({ directory: true, multiple: false, title: "选择串口日志目录" });
              if (typeof picked === "string") update({ serialLogDir: picked });
            }}
          >
            选择…
          </button>
          {settings.serialLogDir && (
            <button className="btn ghost" onClick={() => update({ serialLogDir: "" })}>
              清空
            </button>
          )}
        </div>
      </div>

      <div className="field">
        <label>串口日志格式</label>
        <select
          value={settings.serialLogMode || "raw"}
          onChange={(e) => update({ serialLogMode: e.target.value })}
        >
          <option value="raw">原始字节（不改动任何内容，适合固件协议/后续解析）</option>
          <option value="timestamped">带时间戳文本（每行加 [日期 时间]，控制字符转义为 \xNN）</option>
        </select>
      </div>

      <div className="switch">
        <span>光标闪烁</span>
        <input
          type="checkbox"
          checked={settings.cursorBlink}
          onChange={(e) => update({ cursorBlink: e.target.checked })}
        />
      </div>

      <div className="hint">
        提示：终端内可用 <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>C</kbd> 复制、
        <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>V</kbd> 粘贴；字号与主题改动会立即应用到所有已打开的终端。
      </div>

      <div className="hint" style={{ opacity: 0.7 }}>
        TomatoSSH {version ? `v${version}` : ""} · 版本号由 git tag 决定（CI 打包时自动同步）
      </div>
    </Modal>
  );
}
