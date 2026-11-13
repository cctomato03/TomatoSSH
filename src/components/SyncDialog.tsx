import { useEffect, useState } from "react";
import { CloudUpload, CloudDownload, RefreshCw, LogOut, ShieldCheck } from "lucide-react";
import { api } from "../lib/api";
import type { SyncStateDto } from "../lib/types";
import { useUi } from "../store/useUi";
import { useVault } from "../store/useVault";
import { Modal } from "./Modal";
import { PasswordInput } from "./PasswordInput";

export function SyncDialog() {
  const open = useUi((s) => s.syncOpen);
  const setOpen = useUi((s) => s.setSyncOpen);
  const settings = useVault((s) => s.settings);
  const update = useVault((s) => s.updateSettings);
  const refresh = useVault((s) => s.refreshFromServer);

  const [state, setState] = useState<SyncStateDto | null>(null);
  const [serverUrl, setServerUrl] = useState("");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [mode, setMode] = useState<"login" | "register">("login");
  const [busy, setBusy] = useState<string | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    setServerUrl(settings.sync.serverUrl || "http://127.0.0.1:8787");
    setEmail(settings.sync.email || "");
    setErr(null);
    setMsg(null);
    api
      .syncState()
      .then(setState)
      .catch((e) => setErr(String(e)));
  }, [open, settings.sync.serverUrl, settings.sync.email]);

  if (!open) return null;
  const close = () => setOpen(false);

  const run = async (label: string, fn: () => Promise<void>) => {
    setBusy(label);
    setErr(null);
    setMsg(null);
    try {
      await fn();
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(null);
    }
  };

  const doAuth = () =>
    run(mode === "login" ? "登录中…" : "注册中…", async () => {
      const fn = mode === "login" ? api.syncLogin : api.syncRegister;
      const s = await fn(serverUrl.trim(), email.trim(), password);
      setState(s);
      setPassword("");
      await refresh();
      setMsg(mode === "login" ? "登录成功，可以开始同步了" : "注册成功，已自动登录");
    });

  const doSync = (kind: "now" | "push" | "pull") =>
    run("同步中…", async () => {
      const fn = kind === "now" ? api.syncNow : kind === "push" ? api.syncPush : api.syncPull;
      const r = await fn();
      setState(await api.syncState());
      await refresh();
      setMsg(`${r.message}（版本 ${r.revision}）`);
    });

  const doLogout = () =>
    run("退出中…", async () => {
      await api.syncLogout();
      setState(await api.syncState());
      setMsg("已退出同步账号，本机数据保留");
    });

  const loggedIn = state?.loggedIn ?? false;

  return (
    <Modal title="云同步" onClose={close} footer={<button className="btn primary" onClick={close}>关闭</button>}>
      <div className="hint">
        <ShieldCheck size={12} style={{ verticalAlign: -2, marginRight: 4 }} />
        连接信息（含密码）在本机用 AES-256-GCM 加密后才上传，密钥由账号密码通过 PBKDF2 派生，
        <b>服务端拿不到明文</b>。多台设备使用同一账号同步时会按修改时间自动合并。
      </div>

      {loggedIn ? (
        <>
          <div className="field">
            <label>同步状态</label>
            <div className="hint" style={{ lineHeight: 1.9 }}>
              账号：<b>{state?.email}</b>
              <br />
              服务器：{state?.serverUrl}
              <br />
              云端版本：{state?.revision ?? "—"}
              <br />
              上次同步：{state?.lastSyncAt ? new Date(state.lastSyncAt).toLocaleString() : "从未"}
            </div>
          </div>

          <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
            <button className="btn primary" disabled={!!busy} onClick={() => doSync("now")}>
              <RefreshCw size={13} className={busy === "同步中…" ? "spin" : undefined} /> 立即双向同步
            </button>
            <button className="btn" disabled={!!busy} onClick={() => doSync("push")}>
              <CloudUpload size={13} /> 上传到云端
            </button>
            <button className="btn" disabled={!!busy} onClick={() => doSync("pull")}>
              <CloudDownload size={13} /> 从云端拉取
            </button>
            <button className="btn danger" disabled={!!busy} onClick={doLogout}>
              <LogOut size={13} /> 退出登录
            </button>
          </div>

          <div className="switch">
            <span>启动时自动同步</span>
            <input
              type="checkbox"
              checked={settings.sync.autoSync}
              onChange={(e) =>
                update({ sync: { ...settings.sync, autoSync: e.target.checked } })
              }
            />
          </div>
        </>
      ) : (
        <>
          <div className="seg">
            <button className={mode === "login" ? "on" : undefined} onClick={() => setMode("login")}>
              登录已有账号
            </button>
            <button className={mode === "register" ? "on" : undefined} onClick={() => setMode("register")}>
              注册新账号
            </button>
          </div>
          <div className="field">
            <label>同步服务器地址</label>
            <input
              value={serverUrl}
              onChange={(e) => setServerUrl(e.target.value)}
              placeholder="https://ssh-sync.example.com"
              spellCheck={false}
            />
          </div>
          <div className="field">
            <label>邮箱</label>
            <input value={email} onChange={(e) => setEmail(e.target.value)} placeholder="you@example.com" spellCheck={false} />
          </div>
          <div className="field">
            <label>密码（同时用于加密云端数据，请牢记）</label>
            <PasswordInput
              value={password}
              onChange={setPassword}
              onKeyDown={(e) => {
                if (e.key === "Enter") doAuth();
              }}
              placeholder="至少 6 位"
              title="显示明文密码"
            />
          </div>
          <button className="btn primary block" disabled={!!busy} onClick={doAuth}>
            {busy ?? (mode === "login" ? "登录" : "注册并登录")}
          </button>
          <div className="hint">
            需要先在服务器上部署同步服务：执行 <code>docker compose up -d</code>（详见 README）。
          </div>
        </>
      )}

      {busy && <div className="hint">{busy}</div>}
      {msg && <div className="hint" style={{ color: "var(--success)" }}>{msg}</div>}
      {err && <div className="hint" style={{ color: "var(--danger)", whiteSpace: "pre-wrap" }}>{err}</div>}
    </Modal>
  );
}
