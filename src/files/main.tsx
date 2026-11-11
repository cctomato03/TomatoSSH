import { createRoot } from "react-dom/client";
import { FileManager } from "../components/FileManager";
import "../styles/global.css";
import "../styles/filemanager.css";

const container = document.getElementById("root");
if (!container) throw new Error("找不到 #root 容器");

createRoot(container).render(<FileManager />);
