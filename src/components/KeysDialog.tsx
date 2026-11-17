import { useState } from "react";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { FileKey, KeyRound, Pencil, Plus, Trash2 } from "lucide-react";
import { api } from "../lib/api";
import type { SshKey } from "../lib/types";
import { useVault } from "../store/useVault";
import { Modal } from "./Modal";
import { PasswordInput } from "./PasswordInput";

interface Props {
  onClose: () => void;
}

const emptyKey = (): SshKey => ({
  id: "",
  name: "",
  content: "",
  passphrase: "",
  keyType: "",
  fingerprint: "",
  updatedAt: 0,
  deleted: false,
});

/**
 * 密钥库：把 SSH 私钥当成可复用资源统一管理（存加密保险库、随云同步），
 * 连接里只需要选一把密钥即可，不用每条连接都导入一次。
 */
export function KeysDialog({ onClose }: Props) {
  const keys = useVault((s) => s.keys);
  const saveKey = useVault((s) => s.saveKey);
  const deleteKey = useVault((s) => s.deleteKey);

  const [editing, setEditing] = useState<SshKey>(emptyKey());
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const patch = (p: Partial<SshKey>) => setEditing((f) => ({ ...f, ...p }));

  /** 选文件 → 读取内容与指纹 */
  const pickFile = async () => {
    setError(null);
    const picked = await openFileDialog({
      multiple: false,
      directory: false,
      title: "选择 SSH 私钥文件（不是 .pub 公钥）",
    });
    if (typeof picked !== "string") return;
    setBusy(true);
    try {
      const r = await api.importPrivateKey(picked, editing.passphrase || null);
      patch({
        content: r.content,
        keyType: r.keyType,
        fingerprint: r.fingerprint,
        name: editing.name.trim() || r.name,
      });
      if (r.encrypted && !editing.passphrase) {
        setError("该私钥已用口令加密，请在下面「私钥口令」里填写后再保存");
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const submit = async () => {
    setError(null);
    if (!editing.name.trim()) return setError("请填写密钥名称，例如「生产服务器通用密钥」");
    if (!editing.content.trim()) return setError("请先「选择私钥文件」导入内容");
    setBusy(true);
    try {
      await saveKey({ ...editing, name: editing.name.trim(), passphrase: editing.passphrase || null });
      setEditing(emptyKey());
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal
      title="密钥库"
      onClose={onClose}
      wide
      footer={
        <button className="btn primary" onClick={onClose}>
          关闭
        </button>
      }
    >
      <div className="hint">
        私钥在这里统一管理：**内容**加密存在保险库并随云同步，多条连接只需要「选一把密钥」，换电脑/换系统也不用重新导入。
      </div>

      {error && (
        <div className="hint" style={{ color: "var(--danger)", whiteSpace: "pre-wrap" }}>
          {error}
        </div>
      )}

      <div className="field">
        <label>{editing.id ? "编辑密钥" : "新建密钥"}</label>
        <div className="field-row">
          <input
            placeholder="名称，例如：生产服务器通用密钥"
            value={editing.name}
            onChange={(e) => patch({ name: e.target.value })}
          />
          <button className="btn" onClick={pickFile} disabled={busy}>
            <FileKey size={13} /> {editing.content ? "重新选择文件…" : "选择私钥文件…"}
          </button>
        </div>

        {editing.content && (
          <div className="key-card">
            <span className="key-card-main">
              <b>{editing.name || "（未命名）"}</b>
              <span className="hint">
                {[editing.keyType, editing.fingerprint].filter(Boolean).join(" · ") || "已读取内容"}
              </span>
            </span>
          </div>
        )}

        <div className="field" style={{ marginTop: 8 }}>
          <label>私钥口令（可选，会一起加密保存/同步）</label>
          <PasswordInput
            value={editing.passphrase ?? ""}
            onChange={(v) => patch({ passphrase: v })}
            placeholder="私钥有 passphrase 时填写"
            title="显示明文口令"
          />
        </div>

        <div style={{ display: "flex", gap: 8, justifyContent: "flex-end" }}>
          {editing.id && (
            <button className="btn ghost" onClick={() => setEditing(emptyKey())}>
              取消编辑
            </button>
          )}
          <button className="btn primary" onClick={() => void submit()} disabled={busy}>
            <Plus size={13} /> {editing.id ? "保存修改" : "添加到密钥库"}
          </button>
        </div>
      </div>

      <div className="field">
        <label>
          <KeyRound size={11} /> 密钥库（{keys.length}）
        </label>
        <div className="group-list" style={{ maxHeight: 260 }}>
          {keys.length === 0 && (
            <div className="hint" style={{ padding: "8px 10px" }}>
              暂无密钥，先在上面导入一个
            </div>
          )}
          {keys.map((k) => (
            <div key={k.id} className="snippet-item">
              <span className="snippet-main">
                <b>{k.name}</b>
                <span className="snippet-cmd">
                  {[k.keyType, k.fingerprint, k.passphrase ? "带口令" : null]
                    .filter(Boolean)
                    .join(" · ")}
                </span>
              </span>
              <button
                className="icon-btn"
                title="编辑"
                onClick={() => {
                  setEditing({ ...k, passphrase: k.passphrase ?? "" });
                  setError(null);
                }}
              >
                <Pencil size={13} />
              </button>
              <button
                className="icon-btn"
                title="删除"
                onClick={async () => {
                  const used = useVault
                    .getState()
                    .hosts.filter((h) => h.auth.keyId === k.id).length;
                  const msg = used
                    ? `「${k.name}」正被 ${used} 条连接使用，删除后这些连接将无法用密钥登录，确定删除？`
                    : `删除密钥「${k.name}」？`;
                  if (!window.confirm(msg)) return;
                  await deleteKey(k.id);
                  if (editing.id === k.id) setEditing(emptyKey());
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
