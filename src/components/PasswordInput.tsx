import { useState, type KeyboardEvent } from "react";
import { Eye, EyeOff } from "lucide-react";

interface Props {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  autoFocus?: boolean;
  /** 默认是否明文显示 */
  defaultVisible?: boolean;
  title?: string;
  onKeyDown?: (e: KeyboardEvent<HTMLInputElement>) => void;
}

/**
 * 密码输入框 + 右侧「显示明文」按钮。
 * 点击眼睛图标在 密码 / 明文 之间切换，再次点击隐藏。
 */
export function PasswordInput({
  value,
  onChange,
  placeholder,
  autoFocus,
  defaultVisible = false,
  title = "显示明文",
  onKeyDown,
}: Props) {
  const [visible, setVisible] = useState(defaultVisible);

  return (
    <div className="pw-wrap">
      <input
        type={visible ? "text" : "password"}
        value={value}
        placeholder={placeholder}
        autoFocus={autoFocus}
        spellCheck={false}
        autoComplete="off"
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={onKeyDown}
      />
      <button
        type="button"
        className={`pw-toggle${visible ? " on" : ""}`}
        title={visible ? "隐藏密码" : title}
        aria-label={visible ? "隐藏密码" : title}
        onClick={() => setVisible((v) => !v)}
      >
        {visible ? <EyeOff size={14} /> : <Eye size={14} />}
      </button>
    </div>
  );
}
