import { useEffect, useMemo, useRef, useState } from "react";
import { Eraser } from "lucide-react";

/** 缓冲区上限（保留最近这么多字节） */
const MAX_BYTES = 64 * 1024;
/** 视图里最多渲染的字节数（太多会卡） */
const VIEW_BYTES = 4096;
const BYTES_PER_ROW = 16;

interface Props {
  sessionId: string;
  /** 订阅 term-data 事件（两个视图可以同时订阅） */
  subscribe: (sessionId: string, cb: (chunk: Uint8Array) => void) => Promise<() => void>;
  /** 是否有新数据时自动滚到底部 */
  className?: string;
}

function toHex(n: number, width: number): string {
  return n.toString(16).toUpperCase().padStart(width, "0");
}

/**
 * 串口 HEX 收发视图：把收到的原始字节按 16 字节一行显示（偏移 + HEX + ASCII）。
 * 与 xterm 视图互斥显示，但两边都会持续接收数据。
 */
export function SerialHexView({ sessionId, subscribe, className }: Props) {
  const bufRef = useRef<Uint8Array>(new Uint8Array(0));
  const [version, setVersion] = useState(0);
  const [autoScroll, setAutoScroll] = useState(true);
  const dirtyRef = useRef(false);
  const scrollerRef = useRef<HTMLDivElement>(null);

  // 收集数据（限流：100ms 合并一次重渲染）
  useEffect(() => {
    let un: (() => void) | undefined;
    let disposed = false;
    void subscribe(sessionId, (chunk) => {
      const prev = bufRef.current;
      const next = new Uint8Array(prev.length + chunk.length);
      next.set(prev, 0);
      next.set(chunk, prev.length);
      bufRef.current = next.length > MAX_BYTES ? next.slice(next.length - MAX_BYTES) : next;
      dirtyRef.current = true;
    })
      .then((u) => {
        if (disposed) u();
        else un = u;
      })
      .catch(() => undefined);

    const timer = window.setInterval(() => {
      if (dirtyRef.current) {
        dirtyRef.current = false;
        setVersion((v) => v + 1);
      }
    }, 100);

    return () => {
      disposed = true;
      window.clearInterval(timer);
      un?.();
    };
  }, [sessionId, subscribe]);

  const rows = useMemo(() => {
    void version;
    const data = bufRef.current;
    const start = Math.max(0, data.length - VIEW_BYTES);
    // 从整行边界开始
    const alignedStart = start - (start % BYTES_PER_ROW);
    const slice = data.subarray(alignedStart);
    const out: Array<{ offset: number; hex: string[]; ascii: string }> = [];
    for (let i = 0; i < slice.length; i += BYTES_PER_ROW) {
      const chunk = slice.subarray(i, i + BYTES_PER_ROW);
      const hex: string[] = [];
      let ascii = "";
      for (let j = 0; j < BYTES_PER_ROW; j++) {
        if (j < chunk.length) {
          hex.push(toHex(chunk[j], 2));
          const c = chunk[j];
          ascii += c >= 0x20 && c <= 0x7e ? String.fromCharCode(c) : ".";
        } else {
          hex.push("  ");
        }
      }
      out.push({ offset: alignedStart + i, hex, ascii });
    }
    return out;
  }, [version]);

  useEffect(() => {
    if (!autoScroll) return;
    const el = scrollerRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [rows, autoScroll]);

  const total = bufRef.current.length;

  return (
    <div className={`hexview${className ? ` ${className}` : ""}`}>
      <div className="hexview-head">
        <span>
          十六进制视图 · 已收 {total} 字节
          {total > VIEW_BYTES ? `（仅显示最近 ${VIEW_BYTES} 字节）` : ""}
        </span>
        <label className="hexview-auto">
          <input
            type="checkbox"
            checked={autoScroll}
            onChange={(e) => setAutoScroll(e.target.checked)}
          />
          自动滚动
        </label>
        <button
          className="hexview-clear"
          onClick={() => {
            bufRef.current = new Uint8Array(0);
            setVersion((v) => v + 1);
          }}
        >
          <Eraser size={12} /> 清空
        </button>
      </div>
      <div className="hexview-body" ref={scrollerRef}>
        {rows.length === 0 ? (
          <div className="hexview-empty">还没有收到数据</div>
        ) : (
          rows.map((r) => (
            <div className="hexview-row" key={r.offset}>
              <span className="hexview-offset">{toHex(r.offset, 8)}</span>
              <span className="hexview-hex">
                {r.hex.map((h, i) => (
                  <i key={i} className={i === 7 ? "gap" : undefined}>
                    {h}
                  </i>
                ))}
              </span>
              <span className="hexview-ascii">{r.ascii}</span>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
