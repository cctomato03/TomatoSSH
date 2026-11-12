import { useEffect, useState } from "react";
import { Pencil, Plus, Trash2, Zap } from "lucide-react";
import type { Snippet } from "../lib/types";
import { useVault } from "../store/useVault";
import { Modal } from "./Modal";

interface Props {
  /** 点某个指令的「编辑」时带出来；null 表示新建 */
  snippet: Snippet | null;
  onClose: () => void;
}

const emptySnippet = (): Snippet => ({
  id: "",
  name: "",
  command: "",
  autoRun: true,
  updatedAt: 0,
  deleted: false,
});

export function SnippetsDialog({ snippet, onClose }: Props) {
  const snippets = useVault((s) => s.snippets);
  const saveSnippet = useVault((s) => s.saveSnippet);
  const deleteSnippet = useVault((s) => s.deleteSnippet);

  const [editing, setEditing] = useState<Snippet>(snippet ?? emptySnippet());
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    setEditing(snippet ?? emptySnippet());
    setError(null);
  }, [snippet]);

  const patch = (p: Partial<Snippet>) => setEditing((f) => ({ ...f, ...p }));

  const submit = async () => {
    if (!editing.name.trim()) return setError("请填写指令名称");
    if (!editing.command.trim()) return setError("请填写指令内容");
    setSaving(true);
    try {
      await saveSnippet({
        ...editing,
        name: editing.name.trim(),
      });
      setEditing(emptySnippet());
      setError(null);
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <Modal
      title="快捷指令"
      onClose={onClose}
      wide
      footer={
        <button className="btn primary" onClick={onClose}>
          关闭
        </button>
      }
    >
      <div className="hint">
        快捷指令会显示在终端上方，点一下把内容写进当前终端；勾选「自动回车」则粘贴后直接执行。
        指令存在加密保险库里，并随云同步一起同步。
      </div>

      {error && <div className="hint" style={{ color: "var(--danger)" }}>{error}</div>}

      <div className="field">
        <label>{editing.id ? "编辑指令" : "新建指令"}</label>
        <div className="field-row">
          <input
            placeholder="名称，例如：查看磁盘"
            value={editing.name}
            onChange={(e) => patch({ name: e.target.value })}
          />
          <label className="switch" style={{ padding: 0 }}>
            <span>自动回车执行</span>
            <input
              type="checkbox"
              checked={editing.autoRun}
              onChange={(e) => patch({ autoRun: e.target.checked })}
            />
          </label>
        </div>
        <textarea
          rows={3}
          placeholder="要发送到终端的内容，可多行；例如 df -h"
          value={editing.command}
          onChange={(e) => patch({ command: e.target.value })}
          style={{ resize: "vertical", fontFamily: "ui-monospace, Consolas, monospace" }}
        />
        <div style={{ display: "flex", gap: 8, justifyContent: "flex-end" }}>
          {editing.id && (
            <button
              className="btn ghost"
              onClick={() => setEditing(emptySnippet())}
              title="放弃编辑"
            >
              取消编辑
            </button>
          )}
          <button className="btn primary" onClick={submit} disabled={saving}>
            <Plus size={13} /> {editing.id ? "保存修改" : "添加指令"}
          </button>
        </div>
      </div>

      <div className="field">
        <label>
          <Zap size={11} /> 已有指令（{snippets.length}）
        </label>
        <div className="group-list" style={{ maxHeight: 240 }}>
          {snippets.length === 0 && (
            <div className="hint" style={{ padding: "8px 10px" }}>
              暂无，先在上面添加一条
            </div>
          )}
          {snippets.map((s) => (
            <div
              key={s.id}
              style={{
                display: "flex",
                alignItems: "center",
                gap: 8,
                padding: "6px 10px",
                borderBottom: "1px solid var(--border)",
              }}
            >
              <span style={{ flex: 1, minWidth: 0 }}>
                <b>{s.name}</b>
                <span
                  className="hint"
                  style={{
                    display: "block",
                    overflow: "hidden",
                    textOverflow: "ellipsis",
                    whiteSpace: "nowrap",
                    fontFamily: "ui-monospace, Consolas, monospace",
                  }}
                >
                  {s.command}
                  {s.autoRun ? " ⏎" : ""}
                </span>
              </span>
              <button className="icon-btn" title="编辑" onClick={() => setEditing(s)}>
                <Pencil size={13} />
              </button>
              <button
                className="icon-btn"
                title="删除"
                onClick={async () => {
                  if (window.confirm(`删除快捷指令「${s.name}」？`)) {
                    await deleteSnippet(s.id);
                    if (editing.id === s.id) setEditing(emptySnippet());
                  }
                }}
              >
                <Trash2 size={13} />
              </button>
            </div>
          ))}
        </div>
      </div>
    </Modal>
  );
}
