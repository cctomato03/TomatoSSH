import { useEffect, useState } from "react";
import { useUi } from "../store/useUi";
import { useVault } from "../store/useVault";
import { Modal } from "./Modal";

export function GroupDialog() {
  const { open, group, parentId } = useUi((s) => s.groupDialog);
  const close = useUi((s) => s.closeGroup);
  const groups = useVault((s) => s.groups);
  const saveGroup = useVault((s) => s.saveGroup);

  const [name, setName] = useState("");
  const [parent, setParent] = useState<string>("");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    setName(group?.name ?? "");
    setParent((group?.parentId ?? parentId) ?? "");
    setError(null);
  }, [open, group, parentId]);

  if (!open) return null;

  const submit = async () => {
    if (!name.trim()) return setError("请填写分组名称");
    try {
      await saveGroup({
        id: group?.id,
        name: name.trim(),
        parentId: parent || null,
        color: group?.color ?? null,
      });
      close();
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <Modal
      title={group ? "编辑分组" : "新建分组"}
      onClose={close}
      footer={
        <>
          <button className="btn ghost" onClick={close}>
            取消
          </button>
          <button className="btn primary" onClick={submit}>
            保存
          </button>
        </>
      }
    >
      {error && <div className="hint" style={{ color: "var(--danger)" }}>{error}</div>}
      <div className="field">
        <label>分组名称</label>
        <input value={name} onChange={(e) => setName(e.target.value)} autoFocus placeholder="例如：生产环境" />
      </div>
      <div className="field">
        <label>上级分组</label>
        <select value={parent} onChange={(e) => setParent(e.target.value)}>
          <option value="">（根目录）</option>
          {groups
            .filter((g) => g.id !== group?.id)
            .map((g) => (
              <option key={g.id} value={g.id}>
                {g.name}
              </option>
            ))}
        </select>
      </div>
    </Modal>
  );
}
