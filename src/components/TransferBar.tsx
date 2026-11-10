import { ArrowDownToLine, ArrowUpFromLine, ArrowLeftRight, XCircle } from "lucide-react";
import { formatSize } from "../lib/paths";
import { useTransfers } from "../store/useTransfers";

export function TransferBar() {
  const items = useTransfers((s) => s.items);
  const clearFinished = useTransfers((s) => s.clearFinished);
  const remove = useTransfers((s) => s.remove);

  if (items.length === 0) return null;

  return (
    <div className="transfer-bar">
      {items.map((t) => {
        const pct = t.total > 0 ? Math.min(100, Math.round((t.done / t.total) * 100)) : 0;
        const icon =
          t.direction === "upload" ? (
            <ArrowUpFromLine size={12} />
          ) : t.direction === "download" ? (
            <ArrowDownToLine size={12} />
          ) : (
            <ArrowLeftRight size={12} />
          );
        return (
          <span className="transfer-item" key={t.id}>
            {icon}
            <span title={t.fileName}>{t.fileName}</span>
            <span className="bar">
              <i style={{ width: `${t.status === "done" ? 100 : pct}%` }} />
            </span>
            <span>{t.status === "error" ? "失败" : t.status === "done" ? "完成" : `${pct}%`}</span>
            <span style={{ opacity: 0.7 }}>
              {formatSize(t.done)}
              {t.total ? ` / ${formatSize(t.total)}` : ""}
            </span>
            {t.status !== "running" && (
              <button className="icon-btn" title="移除" onClick={() => remove(t.id)}>
                <XCircle size={12} />
              </button>
            )}
          </span>
        );
      })}
      <button className="btn ghost" style={{ marginLeft: "auto" }} onClick={clearFinished}>
        清理已完成
      </button>
    </div>
  );
}
