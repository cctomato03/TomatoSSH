import { useEffect, useState } from "react";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { KeyRound, FileKey, FolderTree } from "lucide-react";
import type { AuthType, HostDraft } from "../lib/types";
import { useUi } from "../store/useUi";
import { useVault } from "../store/useVault";
import { api } from "../lib/api";
import { Modal } from "./Modal";

const empty = (groupId: string | null): HostDraft => ({
  label: "",
  host: "",
  port: 22,
  username: "",
  auth: { type: "password", password: "", privateKeyPath: null, passphrase: null },
  groupId,
  tags: [],
  notes: "",
  keepaliveSecs: 30,
});

export function HostDialog() {
  const { open, host, groupId } = useUi((s) => s.hostDialog);
  const close = useUi((s) => s.closeHost);
  const groups = useVault((s) => s.groups);
  const saveHost = useVault((s) => s.saveHost);

  const [form, setForm] = useState<HostDraft>(empty(null));
  const [tagText, setTagText] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (!open) return;
    setError(null);
    if (host) {
      setForm({
        id: host.id,
        label: host.label,
        host: host.host,
        port: host.port,
        username: host.username,
        auth: { ...host.auth },
        groupId: host.groupId,
        tags: host.tags,
        notes: host.notes ?? "",
        keepaliveSecs: host.keepaliveSecs,
      });
      setTagText(host.tags.join(", "));
    } else {
      setForm(empty(groupId));
      setTagText("");
    }
  }, [open, host, groupId]);

  if (!open) return null;

  const patch = (p: Partial<HostDraft>) => setForm((f) => ({ ...f, ...p }));
  const patchAuth = (p: Partial<HostDraft["auth"]>) => setForm((f) => ({ ...f, auth: { ...f.auth, ...p } }));

  const pickKey = async () => {
    const picked = await openFileDialog({
      multiple: false,
      directory: false,
      title: "选择 SSH 私钥文件",
    });
    if (typeof picked === "string") patchAuth({ privateKeyPath: picked });
  };

  const submit = async () => {
    setError(null);
    if (!form.host.trim()) return setError("请填写主机地址");
    if (!form.username.trim()) return setError("请填写用户名");
    if (form.auth.type === "password" && !form.auth.password) return setError("请填写登录密码");
    if (form.auth.type === "privateKey" && !form.auth.privateKeyPath)
      return setError("请选择私钥文件");

    setSaving(true);
    try {
      await saveHost({
        ...form,
        label: form.label.trim() || `${form.username}@${form.host}`,
        tags: tagText
          .split(",")
          .map((t) => t.trim())
          .filter(Boolean),
      });
      close();
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <Modal
      title={host ? "编辑连接" : "新建连接"}
      onClose={close}
      footer={
        <>
          <button className="btn ghost" onClick={close}>
            取消
          </button>
          <button className="btn primary" onClick={submit} disabled={saving}>
            {saving ? "保存中…" : "保存"}
          </button>
        </>
      }
    >
      {error && (
        <div className="hint" style={{ color: "var(--danger)" }}>
          {error}
        </div>
      )}

      <div className="field-row">
        <div className="field">
          <label>名称</label>
          <input
            value={form.label}
            placeholder="例如：生产环境 Web"
            onChange={(e) => patch({ label: e.target.value })}
            autoFocus
          />
        </div>
        <div className="field">
          <label>
            <FolderTree size={11} /> 分组
          </label>
          <select
            value={form.groupId ?? ""}
            onChange={(e) => patch({ groupId: e.target.value || null })}
          >
            <option value="">（根目录）</option>
            {groups.map((g) => (
              <option key={g.id} value={g.id}>
                {g.name}
              </option>
            ))}
          </select>
        </div>
      </div>

      <div className="field-row3">
        <div className="field">
          <label>主机（IP 或域名）</label>
          <input
            value={form.host}
            placeholder="192.168.1.10"
            onChange={(e) => patch({ host: e.target.value })}
          />
        </div>
        <div className="field">
          <label>端口</label>
          <input
            type="number"
            value={form.port}
            min={1}
            max={65535}
            onChange={(e) => patch({ port: Number(e.target.value) || 22 })}
          />
        </div>
        <div className="field">
          <label>用户名</label>
          <input value={form.username} placeholder="root" onChange={(e) => patch({ username: e.target.value })} />
        </div>
      </div>

      <div className="field">
        <label>认证方式</label>
        <div className="seg">
          {(["password", "privateKey"] as AuthType[]).map((t) => (
            <button
              key={t}
              className={form.auth.type === t ? "on" : undefined}
              onClick={() => patchAuth({ type: t })}
            >
              {t === "password" ? (
                <>
                  <KeyRound size={12} style={{ verticalAlign: -2, marginRight: 4 }} />
                  密码
                </>
              ) : (
                <>
                  <FileKey size={12} style={{ verticalAlign: -2, marginRight: 4 }} />
                  私钥
                </>
              )}
            </button>
          ))}
        </div>
      </div>

      {form.auth.type === "password" ? (
        <div className="field">
          <label>登录密码</label>
          <input
            type="password"
            value={form.auth.password ?? ""}
            onChange={(e) => patchAuth({ password: e.target.value })}
            placeholder="密码仅保存在本机加密保险库中"
          />
        </div>
      ) : (
        <>
          <div className="field">
            <label>私钥文件</label>
            <div style={{ display: "flex", gap: 6 }}>
              <input
                value={form.auth.privateKeyPath ?? ""}
                onChange={(e) => patchAuth({ privateKeyPath: e.target.value })}
                placeholder="~/.ssh/id_ed25519"
                style={{ flex: 1 }}
              />
              <button className="btn" onClick={pickKey}>
                选择…
              </button>
            </div>
          </div>
          <div className="field">
            <label>私钥口令（可选）</label>
            <input
              type="password"
              value={form.auth.passphrase ?? ""}
              onChange={(e) => patchAuth({ passphrase: e.target.value })}
              placeholder="私钥有 passphrase 时填写"
            />
          </div>
        </>
      )}

      <div className="field-row">
        <div className="field">
          <label>标签（逗号分隔）</label>
          <input value={tagText} onChange={(e) => setTagText(e.target.value)} placeholder="prod, web" />
        </div>
        <div className="field">
          <label>心跳间隔（秒）</label>
          <input
            type="number"
            min={5}
            max={600}
            value={form.keepaliveSecs ?? 30}
            onChange={(e) => patch({ keepaliveSecs: Number(e.target.value) || 30 })}
          />
        </div>
      </div>

      <div className="field">
        <label>备注</label>
        <textarea
          rows={2}
          value={form.notes ?? ""}
          onChange={(e) => patch({ notes: e.target.value })}
          style={{ resize: "vertical" }}
        />
      </div>

      {host?.hostKeyFingerprint && (
        <div className="hint">
          已信任的主机指纹：<code style={{ wordBreak: "break-all" }}>{host.hostKeyFingerprint}</code>
          <div style={{ marginTop: 6 }}>
            <button
              className="btn"
              onClick={async () => {
                if (!window.confirm("清除后下次连接会重新记录服务器指纹，确认？")) return;
                await api.hostClearFingerprint(host.id);
                await useVault.getState().load();
              }}
            >
              清除指纹
            </button>
          </div>
        </div>
      )}
    </Modal>
  );
}
