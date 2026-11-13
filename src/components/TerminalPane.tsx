import { useEffect, useRef, useState } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebLinksAddon } from "@xterm/addon-web-links";
import "@xterm/xterm/css/xterm.css";

import { api, onTermData, onTermExit } from "../lib/api";
import { getTheme } from "../lib/themes";
import type { SerialLines, TabRecord } from "../lib/types";
import { useVault } from "../store/useVault";
import { SerialHexView } from "./SerialHexView";
import { SnippetBar } from "./SnippetBar";
import { open as openDirDialog } from "@tauri-apps/plugin-dialog";
import { useTabs } from "../store/useTabs";
import { Cable, Loader2, PlugZap, RotateCcw, Send } from "lucide-react";

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
  const isSerial = tab.kind === "serial";
  const [hexMode, setHexMode] = useState(false);
  const [lines, setLines] = useState<SerialLines>({ dtr: false, rts: false, cts: false, cd: false });
  const [sendText, setSendText] = useState("");
  const [sendHex, setSendHex] = useState(false);
  const [newline, setNewline] = useState<"none" | "crlf" | "lf" | "cr">("crlf");
  const [history, setHistory] = useState<string[]>([]);
  const [logPath, setLogPath] = useState<string | null>(null);
  const [reconnecting, setReconnecting] = useState(false);
  const reconnectingRef = useRef(false);
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

  /** 打开 / 重新打开会话（三种终端共用；失败时把错误写进终端并置为 error 状态） */
  const openSession = async (term: Terminal) => {
    try {
      fitRef.current?.fit();
    } catch {
      /* ignore */
    }
    const cols = term.cols || 100;
    const rows = term.rows || 30;
    try {
      if (tab.kind === "ssh" && tab.hostId) {
        const host = hosts.find((h) => h.id === tab.hostId);
        patchTab(tab.id, { status: "connecting", error: null });
        term.writeln(
          `\x1b[38;5;244m正在连接 ${host?.username ?? ""}@${host?.host ?? ""}:${host?.port ?? 22} ...\x1b[0m`,
        );
        const banner = await api.sshOpen(tab.id, tab.hostId, cols, rows);
        patchTab(tab.id, { status: "connected", error: null });
        if (banner) term.writeln(`\x1b[38;5;244m${banner}\x1b[0m`);
      } else if (tab.kind === "serial" && tab.serialId) {
        patchTab(tab.id, { status: "connecting", error: null });
        term.writeln(`\x1b[38;5;244m正在打开串口 ...\x1b[0m`);
        const banner = await api.serialOpen(tab.id, tab.serialId);
        patchTab(tab.id, { status: "connected", error: null });
        if (banner) term.writeln(`\x1b[38;5;244m${banner}\x1b[0m`);
      } else {
        patchTab(tab.id, { status: "connecting", error: null });
        const shellInfo = await api.localOpen(tab.id, cols, rows, tab.cwd ?? null);
        patchTab(tab.id, { status: "connected", error: null });
        if (shellInfo) term.writeln(`\x1b[38;5;244m${shellInfo}\x1b[0m`);
      }
    } catch (err) {
      const msg = typeof err === "string" ? err : err instanceof Error ? err.message : String(err);
      setNotice(msg);
      patchTab(tab.id, { status: "error", error: msg });
      term.writeln(`\r\n\x1b[1;31m连接失败：${msg}\x1b[0m`);
    }
  };

  /** 断线 / 结束后「重新连接」 */
  const reconnect = async () => {
    const term = termRef.current;
    if (!term || reconnecting) return;
    setReconnecting(true);
    reconnectingRef.current = true;
    setNotice(null);
    term.writeln(`\r\n\x1b[38;5;244m—— 重新连接 ——\x1b[0m`);
    try {
      await openSession(term);
    } finally {
      setReconnecting(false);
      reconnectingRef.current = false;
      term.focus();
    }
  };

  // 2) 建立会话（只执行一次）
  useEffect(() => {
    if (startedRef.current) return;
    startedRef.current = true;
    const term = termRef.current;
    if (!term) return;

    const unlisteners: Array<() => void> = [];
    let disposed = false;

    const boot = async () => {
      const dataUn = await onTermData(tab.id, (chunk) => term.write(chunk));
      const exitUn = await onTermExit(tab.id, (info) => {
        if (reconnectingRef.current) return; // 旧连接的收尾事件，忽略
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

      await openSession(term);
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

  // 5) 串口：轮询控制线状态（CTS/CD 只有轮询才能拿到）
  useEffect(() => {
    if (!isSerial) return;
    let stopped = false;
    const tick = () => {
      api
        .serialGetLines(tab.id)
        .then((l) => {
          if (!stopped) setLines(l);
        })
        .catch(() => undefined);
    };
    tick();
    const timer = window.setInterval(tick, 2000);
    return () => {
      stopped = true;
      window.clearInterval(timer);
    };
  }, [isSerial, tab.id]);

  const toggleLine = async (line: "dtr" | "rts") => {
    const next = line === "dtr" ? !lines.dtr : !lines.rts;
    try {
      await api.serialSetLine(tab.id, line, next);
      setLines((l) => ({ ...l, [line]: next }));
    } catch (e) {
      setNotice(String(e));
    }
  };

  const doReset = async () => {
    try {
      await api.serialPulseReset(tab.id);
      setLines((l) => ({ ...l, dtr: true, rts: true }));
      termRef.current?.writeln("\x1b[38;5;244m[已发送 DTR/RTS 复位脉冲]\x1b[0m");
    } catch (e) {
      setNotice(String(e));
    }
  };

  /** 串口日志录制开关 */
  const toggleLog = async () => {
    try {
      if (logPath) {
        const stopped = await api.serialStopLog(tab.id);
        setLogPath(null);
        if (stopped) {
          setNotice(`日志已保存：${stopped.path}（${stopped.bytes} 字节）`);
        }
      } else {
        // 日志目录：优先用设置里的，没设置就先让用户挑一个
        let dir = settings.serialLogDir;
        if (!dir) {
          const picked = await openDirDialog({
            title: "选择串口日志目录（按天分文件：<端口>-YYYY-MM-DD.log）",
            directory: true,
            multiple: false,
          });
          if (!picked || typeof picked !== "string") return;
          dir = picked;
          useVault.getState().updateSettings({ serialLogDir: dir });
        }
        const mode = settings.serialLogMode || "raw";
        await api.serialStartLog(tab.id, dir, mode);
        setLogPath(dir);
      }
    } catch (e) {
      setNotice(String(e));
    }
  };

  const doSend = async () => {
    if (!sendText) return;
    try {
      let bytes: Uint8Array;
      if (sendHex) {
        const cleaned = sendText.replace(/0x/gi, "").replace(/[\s,]/g, "");
        if (!/^[0-9a-fA-F]+$/.test(cleaned) || cleaned.length % 2 !== 0) {
          setNotice("HEX 格式不正确，应形如 AA BB 0C（成对十六进制）");
          return;
        }
        bytes = new Uint8Array((cleaned.match(/../g) ?? []).map((h) => parseInt(h, 16)));
      } else {
        const suffix = newline === "crlf" ? "\r\n" : newline === "lf" ? "\n" : newline === "cr" ? "\r" : "";
        bytes = new TextEncoder().encode(sendText + suffix);
      }
      await api.termWriteBytes(tab.id, bytes);
      setHistory((h) => [sendText, ...h.filter((x) => x !== sendText)].slice(0, 10));
      setSendText("");
    } catch (e) {
      setNotice(String(e));
    }
  };

  return (
    <div className="pane" style={{ display: active ? "flex" : "none" }}>
      <div className="term-wrap">
        <SnippetBar sessionId={tab.id} onNotice={setNotice} />
        {isSerial && (
          <div className="serial-toolbar">
            <span className="st-group">
              <Cable size={13} className="st-icon" />
              <button
                className={lines.dtr ? "st-on" : undefined}
                title="切换 DTR 电平"
                onClick={() => void toggleLine("dtr")}
              >
                DTR
              </button>
              <button
                className={lines.rts ? "st-on" : undefined}
                title="切换 RTS 电平"
                onClick={() => void toggleLine("rts")}
              >
                RTS
              </button>
              <span className="st-ind" title="输入侧电平（只读）">
                CTS {lines.cts ? "●" : "○"} · CD {lines.cd ? "●" : "○"}
              </span>
              <button title="DTR/RTS 复位脉冲（开发板常用）" onClick={() => void doReset()}>
                <RotateCcw size={12} /> 复位
              </button>
            </span>

            <span className="st-group">
              <button
                className={hexMode ? "st-on" : undefined}
                title="在文本终端 / 十六进制视图之间切换"
                onClick={() => setHexMode((v) => !v)}
              >
                HEX 视图
              </button>
              <button
                className={logPath ? "st-on" : undefined}
                title={logPath ? `正在录制：${logPath}（点击停止）` : "把收到的数据录制到文件"}
                onClick={() => void toggleLog()}
              >
                {logPath ? "停止录制" : "录制"}
              </button>
            </span>

            <span className="st-send">
              <select value={newline} onChange={(e) => setNewline(e.target.value as typeof newline)}>
                <option value="none">无换行</option>
                <option value="crlf">CRLF</option>
                <option value="lf">LF</option>
                <option value="cr">CR</option>
              </select>
              <input
                value={sendText}
                placeholder={sendHex ? "十六进制，如 AA 55 0D" : "输入要发送的内容，回车发送"}
                onChange={(e) => setSendText(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") void doSend();
                }}
                spellCheck={false}
              />
              <label className="st-hexcheck" title="按十六进制解析输入内容">
                <input type="checkbox" checked={sendHex} onChange={(e) => setSendHex(e.target.checked)} />
                HEX
              </label>
              <button className="st-sendbtn" onClick={() => void doSend()} title="发送">
                <Send size={12} /> 发送
              </button>
              {history.length > 0 && (
                <select
                  value=""
                  title="最近发送"
                  onChange={(e) => {
                    if (e.target.value) setSendText(e.target.value);
                  }}
                >
                  <option value="">最近…</option>
                  {history.map((h, i) => (
                    <option key={`${h}-${i}`} value={h}>
                      {h.length > 24 ? `${h.slice(0, 24)}…` : h}
                    </option>
                  ))}
                </select>
              )}
            </span>
          </div>
        )}

        <div
          className="term-host"
          ref={hostRef}
          style={isSerial && hexMode ? { display: "none" } : undefined}
        />
        {isSerial && hexMode && (
          <SerialHexView sessionId={tab.id} subscribe={onTermData} className="term-hex" />
        )}
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
        {(tab.status === "closed" || tab.status === "error") && (
          <div className="reconnect-bar">
            <span className="rb-text">
              {tab.status === "error"
                ? "连接失败"
                : isSerial
                  ? "串口已关闭"
                  : tab.kind === "local"
                    ? "本地会话已退出"
                    : "连接已断开"}
            </span>
            <button className="btn primary" onClick={() => void reconnect()} disabled={reconnecting}>
              {reconnecting ? (
                <Loader2 size={13} className="spin" />
              ) : (
                <PlugZap size={13} />
              )}
              {reconnecting ? "连接中…" : "重新连接"}
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
