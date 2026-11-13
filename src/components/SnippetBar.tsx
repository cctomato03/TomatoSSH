import { useMemo, useState } from "react";
import { Search, Settings2, X, Zap } from "lucide-react";
import { api } from "../lib/api";
import type { Snippet } from "../lib/types";
import { useVault } from "../store/useVault";
import { SnippetsDialog } from "./SnippetsDialog";

interface Props {
  sessionId: string;
  onNotice?: (msg: string) => void;
}

/**
 * 快捷指令条：点一下就把命令写入当前终端（Shift+点击只粘贴不回车）。
 * SSH / 本地终端 / 串口通用；支持按分组过滤 + 搜索。
 */
export function SnippetBar({ sessionId, onNotice }: Props) {
  const snippets = useVault((s) => s.snippets);
  const [dialog, setDialog] = useState<{ open: boolean; snippet: Snippet | null }>({
    open: false,
    snippet: null,
  });
  const [group, setGroup] = useState("");
  const [searching, setSearching] = useState(false);
  const [query, setQuery] = useState("");

  const groups = useMemo(
    () => Array.from(new Set(snippets.map((s) => s.group).filter((g) => g))).sort(),
    [snippets],
  );

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return snippets.filter((s) => {
      if (group && s.group !== group) return false;
      if (!q) return true;
      return (
        s.name.toLowerCase().includes(q) ||
        s.command.toLowerCase().includes(q) ||
        s.group.toLowerCase().includes(q)
      );
    });
  }, [snippets, group, query]);

  const send = async (snip: Snippet, run: boolean) => {
    const text = snip.command.endsWith("\n") ? snip.command.slice(0, -1) : snip.command;
    const payload = run ? `${text}\r` : text;
    try {
      await api.termWrite(sessionId, payload.replace(/\n/g, "\r"));
    } catch (e) {
      onNotice?.(String(e));
    }
  };

  return (
    <div className="snippet-bar">
      <span className="sb-label">
        <Zap size={13} /> 快捷指令
      </span>

      {groups.length > 0 && (
        <select
          className="sb-group"
          value={group}
          onChange={(e) => setGroup(e.target.value)}
          title="按分组过滤"
        >
          <option value="">全部分组</option>
          {groups.map((g) => (
            <option key={g} value={g}>
              {g}
            </option>
          ))}
        </select>
      )}

      {searching ? (
        <span className="sb-search">
          <Search size={13} />
          <input
            autoFocus
            value={query}
            placeholder="搜索指令名 / 内容"
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape") {
                setSearching(false);
                setQuery("");
              }
            }}
          />
          <button
            title="关闭搜索"
            onClick={() => {
              setSearching(false);
              setQuery("");
            }}
          >
            <X size={13} />
          </button>
        </span>
      ) : null}

      <div className="sb-chips">
        {snippets.length === 0 && <span className="sb-empty">还没有快捷指令</span>}
        {snippets.length > 0 && filtered.length === 0 && (
          <span className="sb-empty">没有匹配的指令</span>
        )}
        {filtered.map((s) => (
          <button
            key={s.id}
            className="sb-chip"
            title={`${s.group ? `[${s.group}] ` : ""}${s.command}${
              s.autoRun ? "（点击=粘贴并回车，Shift+点击=只粘贴）" : "（点击=只粘贴）"
            }`}
            onClick={(e) => void send(s, e.shiftKey ? false : s.autoRun)}
            onContextMenu={(e) => {
              e.preventDefault();
              setDialog({ open: true, snippet: s });
            }}
          >
            {s.group ? <span className="sb-chip-group">{s.group}</span> : null}
            {s.name}
          </button>
        ))}
      </div>

      <div className="sb-actions">
        <button
          title="搜索指令"
          onClick={() => setSearching((v) => !v)}
          className={searching ? "on" : undefined}
        >
          <Search size={14} />
        </button>
        <button
          title="管理快捷指令（新建 / 编辑 / 删除 / 分组）"
          onClick={() => setDialog({ open: true, snippet: null })}
        >
          <Settings2 size={14} />
        </button>
      </div>

      {dialog.open && (
        <SnippetsDialog
          snippet={dialog.snippet}
          onClose={() => setDialog({ open: false, snippet: null })}
        />
      )}
    </div>
  );
}
