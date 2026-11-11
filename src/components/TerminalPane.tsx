import { useEffect, useRef, useState } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebLinksAddon } from "@xterm/addon-web-links";
import "@xterm/xterm/css/xterm.css";

import { api, onTermData, onTermExit } from "../lib/api";
import { getTheme } from "../lib/themes";
import type { TabRecord } from "../lib/types";
import { useVault } from "../store/useVault";
import { useTabs } from "../store/useTabs";
import { Loader2 } from "lucide-react";

interface Props {
  tab: TabRecord;
  active: boolean;
}

export function TerminalPane({ tab, active }: Props) {
  const hostRef = useRef<HTMLDivElement>(null);
  const termRef = useRef<Terminal | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const startedRef = useRef(false);

  const settings = useVault((s) => s.settings);
  const hosts = useVault((s) => s.hosts);
  const patchTab = useTabs((s) => s.patchTab);

  const [notice, setNotice] = useState<string | null>(null);
  const theme = getTheme(settings.theme);

  // 1) 初始化 xterm（只执行一次）
  useEffect(() => {
    const el = hostRef.current;
    if (!el) return;

    const term = new Terminal({
      fontFamily: settings.fontFamily,
      fontSize: settings.fontSize,
      lineHeight: settings.lineHeight,
      cursorBlink: settings.cursorBlink,
      scrollback: settings.scrollback,
      theme: theme.term,
      allowProposedApi: true,
      macOptionIsMeta: true,
      rightClickSelectsWord: false,
      drawBoldTextInBrightColors: true,
      minimumContrastRatio: 1,
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.loadAddon(new WebLinksAddon());
    term.open(el);
    termRef.current = term;
    fitRef.current = fit;

    try {
      fit.fit();
    } catch {
      /* 容器尚未布局 */
    }

    const dataSub = term.onData((data) => {
      api.termWrite(tab.id, data).catch(() => undefined);
    });
    const resizeSub = term.onResize(({ cols, rows }) => {
      api.termResize(tab.id, cols, rows).catch(() => undefined);
    });

    // Ctrl+Shift+C / Ctrl+Shift+V 复制粘贴
    term.attachCustomKeyEventHandler((e) => {
      if (e.type !== "keydown") return true;
      const ctrl = e.ctrlKey || e.metaKey;
      if (ctrl && e.shiftKey && e.code === "KeyC") {
        const sel = term.getSelection();
        if (sel) navigator.clipboard.writeText(sel).catch(() => undefined);
        return false;
      }
      if (ctrl && e.shiftKey && e.code === "KeyV") {
        navigator.clipboard
          .readText()
          .then((text) => api.termWrite(tab.id, text))
          .catch(() => undefined);
        return false;
      }
      return true;
    });

    const ro = new ResizeObserver(() => {
      if (!el.offsetParent) return;
      try {
        fit.fit();
      } catch {
        /* ignore */
      }
    });
    ro.observe(el);

    return () => {
      ro.disconnect();
      dataSub.dispose();
      resizeSub.dispose();
      term.dispose();
      termRef.current = null;
      fitRef.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tab.id]);

  // 2) 建立会话（只执行一次）
  useEffect(() => {
    if (startedRef.current) return;
    startedRef.current = true;
    const term = termRef.current;
    if (!term) return;

    const unlisteners: Array<() => void> = [];
    let disposed = false;

    const boot = async () => {
      try {
        fitRef.current?.fit();
      } catch {
        /* ignore */
      }
      const cols = term.cols || 100;
      const rows = term.rows || 30;

      const dataUn = await onTermData(tab.id, (chunk) => term.write(chunk));
      const exitUn = await onTermExit(tab.id, (info) => {
        patchTab(tab.id, { status: "closed" });
        if (info?.message) term.writeln(`\r\n\x1b[1;31m[连接已关闭] ${info.message}\x1b[0m`);
        else term.writeln(`\r\n\x1b[1;33m[会话已结束${info?.code != null ? `，退出码 ${info.code}` : ""}]\x1b[0m`);
      });
      if (disposed) {
        dataUn();
        exitUn();
        return;
      }
      unlisteners.push(dataUn, exitUn);

      try {
        if (tab.kind === "ssh" && tab.hostId) {
          const host = hosts.find((h) => h.id === tab.hostId);
          patchTab(tab.id, { status: "connecting" });
          term.writeln(`\x1b[38;5;244m正在连接 ${host?.username ?? ""}@${host?.host ?? ""}:${host?.port ?? 22} ...\x1b[0m`);
          const banner = await api.sshOpen(tab.id, tab.hostId, cols, rows);
          patchTab(tab.id, { status: "connected" });
          if (banner) term.writeln(`\x1b[38;5;244m${banner}\x1b[0m`);
        } else if (tab.kind === "serial" && tab.serialId) {
          patchTab(tab.id, { status: "connecting" });
          term.writeln(`\x1b[38;5;244m正在打开串口 ...\x1b[0m`);
          const banner = await api.serialOpen(tab.id, tab.serialId);
          patchTab(tab.id, { status: "connected" });
          if (banner) term.writeln(`\x1b[38;5;244m${banner}\x1b[0m`);
        } else {
          patchTab(tab.id, { status: "connecting" });
          const shellInfo = await api.localOpen(tab.id, cols, rows, tab.cwd ?? null);
          patchTab(tab.id, { status: "connected" });
          if (shellInfo) term.writeln(`\x1b[38;5;244m${shellInfo}\x1b[0m`);
        }
      } catch (err) {
        const msg = typeof err === "string" ? err : err instanceof Error ? err.message : String(err);
        setNotice(msg);
        patchTab(tab.id, { status: "error", error: msg });
        term.writeln(`\r\n\x1b[1;31m连接失败：${msg}\x1b[0m`);
      }
    };

    void boot();

    return () => {
      disposed = true;
      for (const un of unlisteners) un();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tab.id]);

  // 3) 设置变化：字号 / 字体 / 主题实时生效
  useEffect(() => {
    const term = termRef.current;
    if (!term) return;
    term.options.fontFamily = settings.fontFamily;
    term.options.fontSize = settings.fontSize;
    term.options.lineHeight = settings.lineHeight;
    term.options.cursorBlink = settings.cursorBlink;
    term.options.scrollback = settings.scrollback;
    term.options.theme = theme.term;
    try {
      fitRef.current?.fit();
    } catch {
      /* ignore */
    }
  }, [settings.fontFamily, settings.fontSize, settings.lineHeight, settings.cursorBlink, settings.scrollback, theme]);

  // 4) 激活时重新适配并聚焦
  useEffect(() => {
    if (!active) return;
    const raf = requestAnimationFrame(() => {
      try {
        fitRef.current?.fit();
      } catch {
        /* ignore */
      }
      termRef.current?.focus();
    });
    return () => cancelAnimationFrame(raf);
  }, [active]);

  return (
    <div className="pane" style={{ display: active ? "flex" : "none" }}>
      <div className="term-wrap">
        <div className="term-host" ref={hostRef} />
        {notice && (
          <div className="term-notice">
            <span>{notice}</span>
            <button onClick={() => setNotice(null)}>知道了</button>
          </div>
        )}
        {tab.status === "connecting" && (
          <div className="term-badge">
            <Loader2 size={12} className="spin" /> 连接中…
          </div>
        )}
        {tab.status === "closed" && <div className="term-badge">会话已结束</div>}
      </div>
    </div>
  );
}
