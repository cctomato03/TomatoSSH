import { useEffect, useMemo, useState } from "react";
import { Pencil, Plus, Search, Trash2, Zap } from "lucide-react";
import type { Snippet } from "../lib/types";
import { useVault } from "../store/useVault";
import { Modal } from "./Modal";

interface Props {
  /** 点某个指令的「编辑」时带出来；null 表示新建 */
  snippet: Snippet | null;
  onClose: () => void;
}

const emptySnippet = (group = ""): Snippet => ({
  id: "",
  name: "",
  group,
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
  const [query, setQuery] = useState("");

  useEffect(() => {
    setEditing(snippet ?? emptySnippet());
    setError(null);
  }, [snippet]);

  const patch = (p: Partial<Snippet>) => setEditing((f) => ({ ...f, ...p }));

  const groups = useMemo(
    () => Array.from(new Set(snippets.map((s) => s.group).filter((g) => g))).sort(),
    [snippets],
  );

  /** 搜索 + 按分组归类展示 */
  const grouped = useMemo(() => {
    const q = query.trim().toLowerCase();
    const hit = snippets.filter(
      (s) =>
        !q ||
        s.name.toLowerCase().includes(q) ||
        s.command.toLowerCase().includes(q) ||
        s.group.toLowerCase().includes(q),
    );
    const map = new Map<string, Snippet[]>();
    for (const s of hit) {
      const key = s.group || "未分组";
      const list = map.get(key) ?? [];
      list.push(s);
      map.set(key, list);
    }
    return Array.from(map.entries()).sort((a, b) => {
      if (a[0] === "未分组") return 1;
      if (b[0] === "未分组") return -1;
      return a[0].localeCompare(b[0]);
    });
  }, [snippets, query]);

  const submit = async () => {
    if (!editing.name.trim()) return setError("请填写指令名称");
    if (!editing.command.trim()) return setError("请填写指令内容");
    setSaving(true);
    try {
      await saveSnippet({ ...editing, name: editing.name.trim(), group: editing.group.trim() });
      setEditing(emptySnippet(editing.group.trim()));
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
        可以用「分组」把指令归类（例如 MySQL / Docker / 网络），配合终端上方的分组下拉与搜索快速找到。
        指令存在加密保险库里，并随云同步一起同步。
      </div>

      {error && <div className="hint" style={{ color: "var(--danger)" }}>{error}</div>}

      <div className="field">
        <label>{editing.id ? "编辑指令" : "新建指令"}</label>
        <div className="field-row3">
          <input
            placeholder="名称，例如：查看磁盘"
            value={editing.name}
            onChange={(e) => patch({ name: e.target.value })}
          />
          <input
            placeholder="分组（可空）"
            value={editing.group}
            list="snippet-groups"
            onChange={(e) => patch({ group: e.target.value })}
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
        <datalist id="snippet-groups">
          {groups.map((g) => (
            <option key={g} value={g} />
          ))}
        </datalist>
        <textarea
          rows={3}
          placeholder="要发送到终端的内容，可多行；例如 df -h"
          value={editing.command}
          onChange={(e) => patch({ command: e.target.value })}
          style={{ resize: "vertical", fontFamily: "ui-monospace, Consolas, monospace" }}
        />
        <div style={{ display: "flex", gap: 8, justifyContent: "flex-end" }}>
          {editing.id && (
            <button className="btn ghost" onClick={() => setEditing(emptySnippet())} title="放弃编辑">
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
        <div className="sb-search" style={{ marginBottom: 6 }}>
          <Search size={12} />
          <input
            placeholder="搜索名称 / 内容 / 分组"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </div>
        <div className="group-list" style={{ maxHeight: 260 }}>
          {grouped.length === 0 && (
            <div className="hint" style={{ padding: "8px 10px" }}>
              {snippets.length === 0 ? "暂无，先在上面添加一条" : "没有匹配的指令"}
            </div>
          )}
          {grouped.map(([groupName, list]) => (
            <div key={groupName}>
              <div className="ctx-label" style={{ padding: "6px 10px 2px" }}>
                {groupName}（{list.length}）
              </div>
              {list.map((s) => (
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
          ))}
        </div>
      </div>
    </Modal>
  );
}
