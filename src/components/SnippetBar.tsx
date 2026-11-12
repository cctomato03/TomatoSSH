import { useState } from "react";
import { Plus, Settings2, Zap } from "lucide-react";
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
 * SSH / 本地终端 / 串口通用。
 */
export function SnippetBar({ sessionId, onNotice }: Props) {
  const snippets = useVault((s) => s.snippets);
  const [dialog, setDialog] = useState<{ open: boolean; snippet: Snippet | null }>({
    open: false,
    snippet: null,
  });

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
        <Zap size={12} /> 快捷指令
      </span>
      <div className="sb-chips">
        {snippets.length === 0 && <span className="sb-empty">还没有快捷指令</span>}
        {snippets.map((s) => (
          <button
            key={s.id}
            className="sb-chip"
            title={`${s.command}${s.autoRun ? "（点击=粘贴并回车，Shift+点击=只粘贴）" : "（点击=只粘贴）"}`}
            onClick={(e) => void send(s, e.shiftKey ? false : s.autoRun)}
            onContextMenu={(e) => {
              e.preventDefault();
              setDialog({ open: true, snippet: s });
            }}
          >
            {s.name}
          </button>
        ))}
      </div>
      <div className="sb-actions">
        <button title="新建快捷指令" onClick={() => setDialog({ open: true, snippet: null })}>
          <Plus size={13} />
        </button>
        <button
          title="管理快捷指令（右键某个指令可编辑）"
          onClick={() => setDialog({ open: true, snippet: null })}
        >
          <Settings2 size={13} />
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
