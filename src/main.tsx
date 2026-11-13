import { createRoot } from "react-dom/client";
import App from "./App";

const container = document.getElementById("root");
if (!container) throw new Error("找不到 #root 容器");

// 不使用 StrictMode：xterm 实例在二次挂载时会被销毁重建，容易造成会话错乱
createRoot(container).render(<App />);