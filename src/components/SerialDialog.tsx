import { useEffect, useState } from "react";
import { Cable, RefreshCw } from "lucide-react";
import { api } from "../lib/api";
import type { SerialPortInfo, SerialProfile } from "../lib/types";
import { useVault } from "../store/useVault";
import { Modal } from "./Modal";

const BAUD_PRESETS = [1200, 2400, 4800, 9600, 19200, 38400, 57600, 115200, 230400, 460800, 921600];

const emptyProfile = (): SerialProfile => ({
  id: "",
  label: "",
  portName: "",
  baudRate: 115200,
  dataBits: 8,
  parity: "none",
  stopBits: 1,
  flowControl: "none",
  autoReconnect: true,
  updatedAt: 0,
  deleted: false,
});

interface Props {
  /** 要编辑的配置；null 表示新建 */
  profile: SerialProfile | null;
  onClose: () => void;
}

export function SerialDialog({ profile, onClose }: Props) {
  const saveSerial = useVault((s) => s.saveSerial);
  const [form, setForm] = useState<SerialProfile>(profile ?? emptyProfile());
  const [ports, setPorts] = useState<SerialPortInfo[]>([]);
  const [loadingPorts, setLoadingPorts] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [probing, setProbing] = useState(false);
  const [probeInfo, setProbeInfo] = useState<string | null>(null);

  useEffect(() => {
    setForm(profile ?? emptyProfile());
  }, [profile]);

  const refreshPorts = async () => {
    setLoadingPorts(true);
    try {
      const list = await api.serialListPorts();
      setPorts(list);
      // 新建时自动选中第一个可用串口
      setForm((f) => (f.portName || list.length === 0 ? f : { ...f, portName: list[0].name }));
    } catch (e) {
      setError(String(e));
    } finally {
      setLoadingPorts(false);
    }
  };

  useEffect(() => {
    void refreshPorts();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const patch = (p: Partial<SerialProfile>) => setForm((f) => ({ ...f, ...p }));

  /** 自动探测波特率：需要设备正在持续输出数据 */
  const probeBaud = async () => {
    if (!form.portName.trim()) {
      setError("请先选择串口设备");
      return;
    }
    setProbing(true);
    setProbeInfo(null);
    setError(null);
    try {
      const r = await api.serialProbeBaud(form.portName.trim(), 400);
      if (r.baudRate) {
        setForm((f) => ({ ...f, baudRate: r.baudRate as number }));
        setProbeInfo(`探测到 ${r.baudRate}（可信度 ${r.score}%），样本：${r.sample.slice(0, 60)}`);
      } else {
        setProbeInfo(r.sample || "未收到数据，无法判断波特率");
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setProbing(false);
    }
  };

  const submit = async () => {
    setError(null);
    if (!form.portName.trim()) return setError("请选择或填写串口设备");
    if (!form.baudRate || form.baudRate <= 0) return setError("波特率必须大于 0");
    setSaving(true);
    try {
      await saveSerial({ ...form, portName: form.portName.trim(), label: form.label.trim() });
      onClose();
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  };

  const knownPort = ports.some((p) => p.name === form.portName);

  return (
    <Modal
      title={profile ? "编辑串口" : "新建串口连接"}
      onClose={onClose}
      footer={
        <>
          <button className="btn ghost" onClick={onClose}>
            取消
          </button>
          <button className="btn primary" onClick={submit} disabled={saving}>
            {saving ? "保存中…" : "保存"}
          </button>
        </>
      }
    >
      {error && <div className="hint" style={{ color: "var(--danger)" }}>{error}</div>}

      <div className="field-row">
        <div className="field">
          <label>名称</label>
          <input
            value={form.label}
            placeholder="例如：开发板 调试口"
            onChange={(e) => patch({ label: e.target.value })}
            autoFocus
          />
        </div>
        <div className="field">
          <label>
            <Cable size={12} /> 串口设备
          </label>
          <div style={{ display: "flex", gap: 6 }}>
            <select
              value={knownPort ? form.portName : "__manual__"}
              onChange={(e) => {
                const v = e.target.value;
                if (v !== "__manual__") patch({ portName: v });
              }}
              style={{ flex: 1, minWidth: 0 }}
            >
              {ports.length === 0 && <option value="__manual__">（未检测到串口）</option>}
              {ports.map((p) => (
                <option key={p.name} value={p.name}>
                  {p.name}
                  {p.kind === "usb" ? "（USB）" : p.kind === "bluetooth" ? "（蓝牙）" : ""}
                  {p.detail ? ` ${p.detail}` : ""}
                </option>
              ))}
              <option value="__manual__">手动输入…</option>
            </select>
            <button className="btn" onClick={refreshPorts} disabled={loadingPorts} title="刷新串口列表">
              <RefreshCw size={14} className={loadingPorts ? "spin" : undefined} />
            </button>
          </div>
        </div>
      </div>

      {!knownPort && (
        <div className="field">
          <label>手动输入设备路径</label>
          <input
            value={form.portName}
            placeholder="Windows: COM3 ／ Linux: /dev/ttyUSB0 ／ macOS: /dev/cu.usbserial-1420"
            onChange={(e) => patch({ portName: e.target.value })}
            spellCheck={false}
          />
        </div>
      )}

      <div className="field-row3">
        <div className="field">
          <label>波特率</label>
          <div style={{ display: "flex", gap: 4 }}>
          <select
            value={BAUD_PRESETS.includes(form.baudRate) ? String(form.baudRate) : "__custom__"}
            onChange={(e) => {
              const v = e.target.value;
              if (v !== "__custom__") patch({ baudRate: Number(v) });
            }}
          >
            {BAUD_PRESETS.map((b) => (
              <option key={b} value={b}>
                {b}
              </option>
            ))}
            <option value="__custom__">自定义…</option>
          </select>
          <button
            className="btn"
            title="自动探测波特率（设备需要正在输出数据）"
            disabled={probing}
            onClick={() => void probeBaud()}
          >
            {probing ? "探测中…" : "探测"}
          </button>
          </div>
        </div>
        <div className="field">
          <label>数据位</label>
          <select value={form.dataBits} onChange={(e) => patch({ dataBits: Number(e.target.value) })}>
            {[5, 6, 7, 8].map((b) => (
              <option key={b} value={b}>
                {b}
              </option>
            ))}
          </select>
        </div>
        <div className="field">
          <label>校验位</label>
          <select value={form.parity} onChange={(e) => patch({ parity: e.target.value })}>
            <option value="none">None</option>
            <option value="odd">Odd</option>
            <option value="even">Even</option>
          </select>
        </div>
      </div>

      <div className="field-row">
        <div className="field">
          <label>停止位</label>
          <select value={form.stopBits} onChange={(e) => patch({ stopBits: Number(e.target.value) })}>
            <option value={1}>1</option>
            <option value={2}>2</option>
          </select>
        </div>
        <div className="field">
          <label>流控</label>
          <select value={form.flowControl} onChange={(e) => patch({ flowControl: e.target.value })}>
            <option value="none">无</option>
            <option value="software">软件（XON/XOFF）</option>
            <option value="hardware">硬件（RTS/CTS）</option>
          </select>
        </div>
      </div>

      {probeInfo && <div className="hint" style={{ color: "var(--success)" }}>{probeInfo}</div>}

      {!BAUD_PRESETS.includes(form.baudRate) && (
        <div className="field">
          <label>自定义波特率</label>
          <input
            type="number"
            min={1}
            value={form.baudRate}
            onChange={(e) => patch({ baudRate: Number(e.target.value) || 0 })}
          />
        </div>
      )}

      <div className="switch">
        <span>
          掉线后自动重连
          <span className="hint" style={{ display: "block" }}>
            USB 被拔/设备重启后，自动每 1.5 秒尝试重新打开
          </span>
        </span>
        <input
          type="checkbox"
          checked={form.autoReconnect}
          onChange={(e) => patch({ autoReconnect: e.target.checked })}
        />
      </div>

      <div className="hint">
        串口只提供终端交互（没有文件传输）。常见参数：115200 8N1、无流控。若列表里没有你要的设备，可手动输入设备路径。
      </div>
    </Modal>
  );
}
