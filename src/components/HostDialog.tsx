import { useEffect, useState } from "react";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { KeyRound, FileKey, FolderTree } from "lucide-react";
import type { AuthType, HostDraft } from "../lib/types";
import { useUi } from "../store/useUi";
import { useVault } from "../store/useVault";
import { api } from "../lib/api";
import { Modal } from "./Modal";
import { PasswordInput } from "./PasswordInput";

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
  const hosts = useVault((s) => s.hosts);
  const saveHost = useVault((s) => s.saveHost);

  const [form, setForm] = useState<HostDraft>(empty(null));
  const [tagText, setTagText] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  /** 私钥存放方式：vault = 存进保险库（可云同步），path = 本机文件路径 */
  const [keyMode, setKeyMode] = useState<"vault" | "path">("vault");
  const [keyInfo, setKeyInfo] = useState<string | null>(null);

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
      setKeyMode(host.auth.privateKeyPem ? "vault" : "path");
      setKeyInfo(null);
    } else {
      setForm(empty(groupId));
      setTagText("");
      setKeyMode("vault");
      setKeyInfo(null);
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

  /** 导入私钥文件：读取内容存进保险库（跨机器可用的推荐方式） */
  const importKey = async () => {
    setError(null);
    const picked = await openFileDialog({
      multiple: false,
      directory: false,
      title: "选择 SSH 私钥文件（不是 .pub 公钥）",
    });
    if (typeof picked !== "string") return;
    try {
      const r = await api.importPrivateKey(picked, form.auth.passphrase ?? null);
      patchAuth({ privateKeyPem: r.content, privateKeyName: r.name, privateKeyPath: null });
      setKeyMode("vault");
      setKeyInfo(
        [r.keyType, r.fingerprint, r.encrypted ? "带口令" : null].filter(Boolean).join(" · "),
      );
      if (r.encrypted && !form.auth.passphrase) {
        setError("该私钥已用口令加密，请在下方「私钥口令」里填写");
      }
    } catch (e) {
      setError(String(e));
    }
  };

  const submit = async () => {
    setError(null);
    if (!form.host.trim()) return setError("请填写主机地址");
    if (!form.username.trim()) return setError("请填写用户名");
    if (form.auth.type === "password" && !form.auth.password) return setError("请填写登录密码");
    if (form.auth.type === "privateKey") {
      const hasPem = !!(form.auth.privateKeyPem ?? "").trim();
      const hasPath = !!(form.auth.privateKeyPath ?? "").trim();
      if (!hasPem && !hasPath) return setError("请导入私钥文件，或填写本机私钥路径");
    }

    setSaving(true);
    try {
      await saveHost({
        ...form,
        // 只保存当前选择的来源，避免两套路径/内容同时存在造成歧义
        auth: {
          ...form.auth,
          privateKeyPem: keyMode === "vault" ? form.auth.privateKeyPem : null,
          privateKeyName: keyMode === "vault" ? form.auth.privateKeyName : null,
          privateKeyPath: keyMode === "path" ? form.auth.privateKeyPath : null,
        },
        forwards: host?.forwards ?? form.forwards ?? [],
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
            <FolderTree size={12} /> 分组
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
                  <KeyRound size={13} style={{ verticalAlign: -2, marginRight: 4 }} />
                  密码
                </>
              ) : (
                <>
                  <FileKey size={13} style={{ verticalAlign: -2, marginRight: 4 }} />
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
          <PasswordInput
            value={form.auth.password ?? ""}
            onChange={(v) => patchAuth({ password: v })}
            placeholder="密码仅保存在本机加密保险库中"
            title="显示明文密码"
          />
        </div>
      ) : (
        <>
          <div className="field">
            <label>私钥存放方式</label>
            <div className="seg">
              <button
                className={keyMode === "vault" ? "on" : undefined}
                onClick={() => setKeyMode("vault")}
                title="私钥内容加密存进本机保险库，并随云同步到其它设备"
              >
                存进保险库（推荐 · 可云同步）
              </button>
              <button
                className={keyMode === "path" ? "on" : undefined}
                onClick={() => setKeyMode("path")}
                title="只在本机按路径读取，换电脑后需要重新选择"
              >
                使用本机文件路径
              </button>
            </div>
          </div>

          {keyMode === "vault" ? (
            <div className="field">
              <label>私钥内容（保存在加密保险库，随云同步，换电脑 / 换系统都能直接用）</label>
              {(form.auth.privateKeyPem ?? "").trim() ? (
                <div className="key-card">
                  <span className="key-card-main">
                    <b>{form.auth.privateKeyName || "已导入的私钥"}</b>
                    <span className="hint">{keyInfo ?? "已保存在保险库"}</span>
                  </span>
                  <button className="btn ghost" onClick={importKey}>
                    替换…
                  </button>
                  <button
                    className="btn ghost"
                    onClick={() => {
                      patchAuth({ privateKeyPem: null, privateKeyName: null });
                      setKeyInfo(null);
                    }}
                  >
                    移除
                  </button>
                </div>
              ) : (
                <div style={{ display: "flex", gap: 10, alignItems: "center", flexWrap: "wrap" }}>
                  <button className="btn primary" onClick={importKey}>
                    <FileKey size={13} /> 导入私钥文件…
                  </button>
                  <span className="hint">
                    导入的是文件内容（不是路径），会一起加密同步到你的其它设备
                  </span>
                </div>
              )}
            </div>
          ) : (
            <div className="field">
              <label>私钥文件路径</label>
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
              <span className="hint">
                路径只在本机有效，换电脑/换系统后需要重新选择（推荐上面的保险库方式）
              </span>
            </div>
          )}

          <div className="field">
            <label>私钥口令（可选）</label>
            <PasswordInput
              value={form.auth.passphrase ?? ""}
              onChange={(v) => patchAuth({ passphrase: v })}
              placeholder="私钥有 passphrase 时填写"
              title="显示明文口令"
            />
          </div>
        </>
      )}

      <div className="field">
        <label>跳板机（ProxyJump，可选）</label>
        <select
          value={form.jumpHostId ?? ""}
          onChange={(e) => patch({ jumpHostId: e.target.value || null })}
        >
          <option value="">（直连）</option>
          {hosts
            .filter((h) => h.id !== form.id)
            .map((h) => (
              <option key={h.id} value={h.id}>
                {h.label || `${h.username}@${h.host}`}
              </option>
            ))}
        </select>
        <span className="hint">先登录所选连接，再通过它连接本主机（需要目标主机在跳板机可达）。</span>
      </div>

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
