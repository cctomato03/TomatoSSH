import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";

export interface CtxAction {
  kind?: "action";
  label: string;
  icon?: ReactNode;
  danger?: boolean;
  onClick: () => void;
}
export interface CtxSep {
  kind: "sep";
}
export interface CtxHeader {
  kind: "header";
  label: string;
}

export type CtxItem = CtxAction | CtxSep | CtxHeader;

interface Props {
  x: number;
  y: number;
  items: CtxItem[];
  onClose: () => void;
}

export function ContextMenu({ x, y, items, onClose }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ left: x, top: y });

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const rect = el.getBoundingClientRect();
    const left = Math.min(x, window.innerWidth - rect.width - 8);
    const top = Math.min(y, window.innerHeight - rect.height - 8);
    setPos({ left: Math.max(8, left), top: Math.max(8, top) });
  }, [x, y]);

  useEffect(() => {
    const close = () => onClose();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("mousedown", close);
    window.addEventListener("keydown", onKey);
    window.addEventListener("blur", close);
    return () => {
      window.removeEventListener("mousedown", close);
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("blur", close);
    };
  }, [onClose]);

  return (
    <div
      className="ctx"
      ref={ref}
      style={{ left: pos.left, top: pos.top }}
      onMouseDown={(e) => e.stopPropagation()}
    >
      {items.map((item, i) => {
        if (item.kind === "sep") return <div className="ctx-sep" key={`sep-${i}`} />;
        if (item.kind === "header")
          return (
            <div className="ctx-label" key={`h-${i}`}>
              {item.label}
            </div>
          );
        return (
          <button
            key={item.label + i}
            className={item.danger ? "danger" : undefined}
            onClick={() => {
              item.onClick();
              onClose();
            }}
          >
            {item.icon}
            {item.label}
          </button>
        );
      })}
    </div>
  );
}
