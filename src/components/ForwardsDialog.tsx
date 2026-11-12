import { useCallback, useEffect, useState } from "react";
import { Play, Plus, Square, Trash2, Waypoints } from "lucide-react";
import { api } from "../lib/api";
import type { ActiveForward, PortForwardRule } from "../lib/types";
import { useVault } from "../store/useVault";
import { Modal } from "./Modal";

interface Props {
  hostId: string;
  onClose: () => void;
}

const emptyRule = (): PortForwardRule => ({
  id: crypto.randomUUID(),
  bindHost: "127.0.0.1",
  bindPort: 0,
  destHost: "127.0.0.1",
  destPort: 0,
  autoStart: false,
});

/** 本地端口转发（-L）：本机 listen → 经 SSH 连到目标 */
export function ForwardsDialog({ hostId, onClose }: Props) {
  const host = useVault((s) => s.hosts.find((h) => h.id === hostId));
  const [active, setActive] = useState<ActiveForward[]>([]);
  const [rule, setRule] = useState<PortForwardRule>(emptyRule());
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);

  const rules = host?.forwards ?? [];

  const refresh = useCallback(async () => {
    try {
      const list = await api.forwardList();
      setActive(list.filter((f) => f.hostId === hostId));
    } catch {
      /* 忽略 */
    }
  }, [hostId]);

  useEffect(() => {
    void refresh();
    const t = window.setInterval(() => void refresh(), 2000);
    return () => window.clearInterval(t);
  }, [refresh]);

  const saveRules = async (next: PortForwardRule[]) => {
    if (!host) return;
    await api.hostSave({
      id: host.id,
      groupId: host.groupId,
      label: host.label,
      host: host.host,
      port: host.port,
      username: host.username,
      auth: host.auth,
      color: host.color,
      tags: host.tags,
      notes: host.notes,
      keepaliveSecs: host.keepaliveSecs,
      jumpHostId: host.jumpHostId ?? null,
      forwards: next,
    });
    await useVault.getState().load();
  };

  const addRule = async () => {
    if (!rule.bindPort || !rule.destPort) {
      setError("监听端口和目标端口都要填（监听端口填 0 表示由系统随机分配）");
      return;
    }
    if (!rule.destHost.trim()) {
      setError("目标地址不能为空");
      return;
    }
    setError(null);
    await saveRules([...rules, { ...rule, destHost: rule.destHost.trim() }]);
    setRule(emptyRule());
  };

  const startRule = async (r: PortForwardRule) => {
    setBusy(r.id);
    setError(null);
    try {
      await api.forwardStart(hostId, r);
      await refresh();
      await useVault.getState().load();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  };

  return (
    <Modal
      title={`端口转发 · ${host?.label || host?.host || ""}`}
      onClose={onClose}
      wide
      footer={<button className="btn primary" onClick={onClose}>关闭</button>}
    >
      <div className="hint">
        本地转发（相当于 <code>ssh -L</code>）：本机监听一个端口，流量经 SSH 隧道送到远端目标。
        例如把远端 <code>127.0.0.1:3306</code>（MySQL）映射到本机 <code>13306</code>，本机工具即可连。
        规则保存在该连接上；勾选「打开连接时自动启动」后，开终端时会自动建立。
      </div>

      {error && <div className="hint" style={{ color: "var(--danger)", whiteSpace: "pre-wrap" }}>{error}</div>}

      <div className="field">
        <label>新增规则</label>
        <div className="field-row3">
          <div className="field">
            <label>目标 主机:端口</label>
            <div style={{ display: "flex", gap: 4 }}>
              <input
                value={rule.destHost}
                placeholder="127.0.0.1"
                onChange={(e) => setRule((r) => ({ ...r, destHost: e.target.value }))}
                style={{ flex: 1, minWidth: 0 }}
              />
              <input
                type="number"
                min={1}
                max={65535}
                value={rule.destPort || ""}
                placeholder="3306"
                onChange={(e) => setRule((r) => ({ ...r, destPort: Number(e.target.value) || 0 }))}
                style={{ width: 88 }}
              />
            </div>
          </div>
          <div className="field">
            <label>本机监听端口</label>
            <input
              type="number"
              min={0}
              max={65535}
              value={rule.bindPort || ""}
              placeholder="0=随机"
              onChange={(e) => setRule((r) => ({ ...r, bindPort: Number(e.target.value) || 0 }))}
            />
          </div>
          <div className="field">
            <label>自动启动</label>
            <label className="switch" style={{ padding: "4px 0" }}>
              <span>开终端时自动建立</span>
              <input
                type="checkbox"
                checked={rule.autoStart}
                onChange={(e) => setRule((r) => ({ ...r, autoStart: e.target.checked }))}
              />
            </label>
          </div>
        </div>
        <div style={{ display: "flex", justifyContent: "flex-end" }}>
          <button className="btn" onClick={addRule}>
            <Plus size={13} /> 添加规则
          </button>
        </div>
      </div>

      <div className="field">
        <label>
          <Waypoints size={11} /> 已保存规则（{rules.length}）
        </label>
        <div className="group-list" style={{ maxHeight: 180 }}>
          {rules.length === 0 && (
            <div className="hint" style={{ padding: "8px 10px" }}>还没有规则</div>
          )}
          {rules.map((r) => {
            const running = active.find(
              (a) => a.destHost === r.destHost && a.destPort === r.destPort,
            );
            return (
              <div
                key={r.id}
                style={{ display: "flex", alignItems: "center", gap: 8, padding: "6px 10px" }}
              >
                <span style={{ flex: 1, minWidth: 0, fontSize: 12 }}>
                  <b>
                    127.0.0.1:{running ? running.bindPort : r.bindPort || "随机"} →{" "}
                    {r.destHost}:{r.destPort}
                  </b>
                  <span className="hint" style={{ display: "block" }}>
                    {running
                      ? `运行中 · 已建立 ${running.connections} 个连接`
                      : r.autoStart
                        ? "未启动（开终端时自动建立）"
                        : "未启动"}
                  </span>
                </span>
                {running ? (
                  <button
                    className="icon-btn"
                    title="停止"
                    onClick={async () => {
                      await api.forwardStop(running.id);
                      await refresh();
                    }}
                  >
                    <Square size={13} />
                  </button>
                ) : (
                  <button
                    className="icon-btn"
                    title="启动"
                    disabled={busy === r.id}
                    onClick={() => void startRule(r)}
                  >
                    <Play size={13} />
                  </button>
                )}
                <button
                  className="icon-btn"
                  title="删除规则"
                  onClick={async () => {
                    if (running) await api.forwardStop(running.id);
                    await saveRules(rules.filter((x) => x.id !== r.id));
                    await refresh();
                  }}
                >
                  <Trash2 size={13} />
                </button>
              </div>
            );
          })}
        </div>
      </div>
    </Modal>
  );
}
