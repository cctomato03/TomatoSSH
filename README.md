# 🍅 TomatoSSH

一个模仿 Termius 的跨平台 SSH / SFTP 桌面客户端，支持 **Windows / macOS / Ubuntu Desktop**。
连接信息保存在本地加密保险库中，也可以部署一个自己的同步服务（Docker 一键起）在多台电脑之间同步。

> 技术栈：**Tauri 2（Rust）+ React 19 + TypeScript + xterm.js**，后端 SSH 使用纯 Rust 的 `russh`，
> 本地终端使用 `portable-pty`，云同步服务端为 `axum + SQLite`（Rust），全部打包成单个二进制。

---

## 功能一览

| # | 需求 | 实现情况 |
|---|------|----------|
| 1 | 连接信息管理与本地持久化，密码 / 密钥登录 | ✅ 连接 + 多级分组管理，数据写入 `vault.bin`（AES-256-GCM 加密）；支持密码认证、私钥认证（含 passphrase） |
| 2 | 远程文件上传下载 + 不同远程主机之间互传 | ✅ 内置 SFTP 双栏文件面板：本地 ⇄ 远程、远程 ⇄ 远程（服务器之间直接对流，不落地本地磁盘） |
| 3 | 分组 + 左侧连接列表 + 右侧终端，多连接并行 | ✅ 左侧为分组树 / 连接列表 / 搜索，右侧为多标签终端，标签可并行打开多个会话 |
| 4 | 字号调整 + 主题切换（内置多套美观主题） | ✅ 设置面板可调字号、行高、字体、光标闪烁、回滚缓冲；内置 **8 套主题**（番茄暗夜 / Nord / Dracula / Gruvbox / One Dark / 东京夜色 / 暖阳浅色 / 素白 GitHub） |
| 5 | 支持操作系统本地终端 | ✅ 侧边栏「本地终端」或在标签栏 `+` 新建，Windows 自动选 `pwsh`/`powershell`/`cmd`，Linux/macOS 使用 `$SHELL` |
| 6 | 连接信息云同步（自建 Docker 服务） | ✅ 自建账号体系（邮箱 + 密码，JWT），`server/` 一键 Docker 部署；**客户端先加密再上传，服务端看不到明文**，多设备按修改时间自动合并 |

其他细节：

- 主机公钥指纹 TOFU 校验（首次连接记录指纹，指纹变化会中断连接并提示，可在连接编辑里清除）
- 传输进度条、面板错误提示、右键菜单（连接 / 编辑 / 复制 / 删除 / 新建子分组…）
- 终端快捷键：`Ctrl/Cmd + Shift + C` 复制、`Ctrl/Cmd + Shift + V` 粘贴、`Ctrl/Cmd + T` 新建本地终端、`Ctrl/Cmd + Shift + W` 关闭标签
- 全部 UI 主题变量联动（侧边栏、面板、弹窗、终端配色一起切换）

---

## 目录结构

```
TomatoSSH/
├── src/                      # 前端（React + TypeScript）
│   ├── components/           # 侧边栏、标签栏、终端、文件面板、各类弹窗
│   ├── lib/                  # IPC 封装、类型定义、主题定义、路径工具
│   ├── store/                # zustand 状态（保险库/标签页/传输/UI）
│   └── styles/global.css     # 全局样式（CSS 变量随主题变化）
├── src-tauri/                # 桌面端后端（Rust）
│   ├── src/
│   │   ├── main.rs           # 程序入口
│   │   ├── lib.rs            # Tauri Builder、命令注册
│   │   ├── commands.rs       # 全部 #[tauri::command]
│   │   ├── vault.rs          # 加密保险库（连接/分组/设置持久化、云端合并）
│   │   ├── crypto.rs         # AES-256-GCM、PBKDF2
│   │   ├── ssh.rs            # SSH 会话与终端流
│   │   ├── sftp.rs           # SFTP 浏览 / 上传 / 下载 / 远程互传
│   │   ├── local.rs          # 本地终端（PTY）
│   │   ├── localfs.rs        # 本地文件浏览
│   │   ├── sync.rs           # 云同步 HTTP 客户端
│   │   ├── models.rs         # 数据模型（serde camelCase）
│   │   └── error.rs          # 统一错误类型
│   ├── capabilities/         # Tauri 权限声明
│   └── tauri.conf.json       # 窗口 / 打包配置
├── server/                   # 云同步服务端（Rust + axum + SQLite + Docker）
│   ├── src/{main,api,auth,db,config,error}.rs
│   ├── Dockerfile
│   ├── docker-compose.yml
│   └── .env.example
└── tools/                    # 开发辅助（编译校验 / 冒烟测试用的 tauri 桩）
```

---

## 开发环境

| 依赖 | 说明 |
|------|------|
| Node.js ≥ 20 + pnpm | 前端构建 |
| Rust ≥ 1.82 | 后端（`rustup` 安装） |
| Windows | 安装 **Visual Studio Build Tools（C++ 生成工具）** + **WebView2 Runtime**（Win10/11 一般自带） |
| macOS | `xcode-select --install` |
| Ubuntu / Debian | `sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev` |

### 本地开发

```bash
pnpm install
pnpm tauri:dev        # 打开开发窗口（前端热更新）
```

### 打包

```bash
pnpm tauri:build
```

产物在 `src-tauri/target/release/bundle/`：

- Windows：`.msi`（WiX）/ `.exe`（NSIS）
- macOS：`.dmg` / `.app`
- Linux：`.deb` / `.AppImage`

> 纯前端调试可以用 `pnpm dev`（但 `invoke` 调用需要 Tauri 运行时，浏览器里会提示）。

### 常用脚本

```bash
pnpm typecheck                     # 前端类型检查
pnpm build                         # 类型检查 + 生产构建
cd src-tauri && cargo check        # 后端编译检查
cd server && cargo run             # 本地启动同步服务（默认 0.0.0.0:8787）
```

---

## 使用说明

1. **新建连接**：左下角「连接」或侧边栏右键 → 填主机 / 端口 / 用户名，选择密码或私钥（可点「选择…」挑私钥文件）。
2. **分组**：左下角「分组」，或右键分组 →「新建子分组」；连接和分组都支持右键重命名 / 删除 / 复制。
3. **打开终端**：双击连接或点行尾 ▶。同一个连接重复打开会复用已存在的标签页。
4. **文件传输**：在 SSH 标签页点右上角文件图标，打开双栏面板：
   - 左侧本地、右侧远程，中间 `→` 上传、`←` 下载；
   - 底部「远程互传」选择另一个已连接的会话，即可把右侧选中的文件直接发送到另一台服务器；
   - 支持多选（`Ctrl/Cmd` 点选）、新建目录、重命名、删除。
5. **本地终端**：侧边栏「本地终端」或标签栏 `+`。
6. **设置**：右上角齿轮 → 主题 / 字体 / 字号 / 行高 / 回滚缓冲 / 光标闪烁，改动对所有已打开终端立即生效。
7. **云同步**：右上角云图标 → 填服务器地址 + 邮箱 + 密码 → 注册或登录 → 「立即双向同步」。

---

## 云同步服务部署

服务端非常轻量（单个静态二进制 + SQLite），可以跑在你的云服务器上。

```bash
cd server
cp .env.example .env
# 生成一个足够随机的密钥并填进 .env
openssl rand -hex 32

docker compose up -d --build
docker compose logs -f
curl http://127.0.0.1:8787/healthz
```

客户端里服务器地址填 `http://你的服务器IP:8787`（建议前面套一层 Nginx/Caddy 上 HTTPS，见下文）。

### 环境变量

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `BIND_ADDR` | `0.0.0.0:8787` | 监听地址 |
| `DATABASE_PATH` | `/data/tomatossh.db` | SQLite 路径（Docker 里挂在 `tomatossh-data` 卷） |
| `JWT_SECRET` | 随机（仅本次运行有效） | **生产必须固定**，否则重启后所有客户端需要重新登录 |
| `ALLOW_REGISTER` | `true` | 部署完成后建议改成 `false` 关闭注册 |
| `TOKEN_TTL_HOURS` | `720` | 登录令牌有效期（小时） |
| `MAX_BLOB_BYTES` | `16777216` | 单个保险库密文大小上限（16 MB） |

### 建议：用 Nginx / Caddy 加 HTTPS

```nginx
server {
    listen 443 ssl http2;
    server_name ssh-sync.example.com;

    ssl_certificate     /etc/letsencrypt/live/ssh-sync.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/ssh-sync.example.com/privkey.pem;

    location / {
        proxy_pass         http://127.0.0.1:8787;
        proxy_set_header   Host $host;
        proxy_set_header   X-Real-IP $remote_addr;
        client_max_body_size 32m;
    }
}
```

### HTTP 接口

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/healthz` | 健康检查 |
| POST | `/api/v1/auth/register` | `{email, password}` → `{token, email}` |
| POST | `/api/v1/auth/login` | `{email, password}` → `{token, email}` |
| GET | `/api/v1/sync/vault` | `Authorization: Bearer <token>` → `{revision, blob, updated_at}` |
| PUT | `/api/v1/sync/vault` | `{blob, base_revision?, device_id}` → `{revision}`；`base_revision` 不匹配返回 409 |

---

## 数据安全说明

- **本地**：所有连接（含密码、私钥路径、备注）序列化后用 **AES-256-GCM** 加密写入
  `vault.bin`，密钥文件 `vault.key`（32 字节随机，Unix 下权限 `0600`）与保险库同目录。
  目录位置：
  - Windows：`%APPDATA%\com.tomatossh.desktop\`
  - macOS：`~/Library/Application Support/com.tomatossh.desktop/`
  - Linux：`~/.config/com.tomatossh.desktop/`
- **云端**：上传前在客户端用 **PBKDF2-HMAC-SHA256(账号密码, salt=邮箱, 12 万次迭代)** 派生密钥，
  再用 AES-256-GCM 加密整个载荷。服务端只存 `base64(密文)`，**既看不到连接信息，也拿不到密钥**。
  因此：**忘记同步密码 = 云端数据无法解密**（服务端也没有重置手段），请妥善保管。
- **合并策略**：每台设备拉取后按实体 `updatedAt` 比较，后写者胜；删除采用墓碑标记（`deleted=true` 参与同步），
  不会出现「在一台设备删掉、另一台又被加回来」。
- **主机指纹 TOFU**：首次连接记录服务器公钥 SHA256 指纹，之后指纹不一致会拒绝连接并提示可能是中间人攻击。

---

## 已知限制 / Roadmap

- SSH 认证目前支持 **密码** 与 **私钥**；`ssh-agent` 转发、跳板机（ProxyJump）、端口转发尚未实现（数据模型已预留）。
- SFTP 传输支持文件与空目录删除，暂不支持目录递归上传/下载。
- 同步为「整库覆盖 + 按时间合并」，尚未做字段级冲突交互提示。
- 还没有会话历史 / 命令片段 / 密钥管理（Termius 的 Snippets、Vault）等增值功能。

---

## 故障排查

| 现象 | 处理 |
|------|------|
| 提示「主机密钥指纹不匹配」 | 服务器重装或换钥了：编辑连接 → 清除指纹 后重连 |
| 终端中文/颜色显示异常 | 设置里换个等宽字体（如 JetBrains Mono / Cascadia Code） |
| 同步报 401 | 重新登录同步账号（`JWT_SECRET` 变了或令牌过期） |
| 同步报 409 | 云端版本比你新，先「从云端拉取」再「上传」 |
| Linux 打包报缺 `webkit2gtk` | 按上面 Ubuntu 依赖列表安装开发包 |
| Windows 编译报找不到链接器 | 安装 Visual Studio Build Tools 的「使用 C++ 的桌面开发」工作负载 |

---

## 开发辅助：无 Tauri 环境时的后端校验

`tools/verify` 通过 `#[path]` 直接复用 `src-tauri/src` 的真实源码，把 tauri 运行时换成一个极小的桩实现，
因此即使机器上暂时装不上 Tauri 构建依赖，也能验证后端逻辑能否编译、能否跑通：

```bash
cd tools/verify
cargo check              # 静态编译校验（模型 / 保险库 / SSH / SFTP / PTY / 同步 / 命令注册）
cargo run --bin smoke_core   # 冒烟测试：加密持久化、软删除、云端合并、本地文件操作
cargo run --bin smoke        # 完整冒烟（额外覆盖本地 PTY 终端与 SSH 连接错误路径）
```

> 该目录仅用于开发期校验，不参与正式打包。

### 本仓库交付时的验证记录

| 项目 | 结果 |
|------|------|
| 前端类型检查 + 生产构建（`pnpm build`） | ✅ 通过（`tsc --noEmit` + vite 产物 620 kB / gzip 174 kB） |
| 后端全部模块编译（`tools/verify` 复用真实源码 + 真实 russh / russh-sftp / portable-pty / reqwest） | ✅ 通过，无错误无警告 |
| 前后端 IPC 契约（35 个命令的命令名 / 参数名 / 注册表） | ✅ 通过（`python scripts/check_contract.py`） |
| 同步服务端编译 | ✅ 通过 |
| 同步服务端运行 + 接口联调（注册 / 登录 / 密码错误 / 上传 / 拉取 / 版本冲突 409 / 重复注册 409） | ✅ 全部符合预期 |
| `tauri dev` / `tauri build` 桌面窗口 | ⚠️ 交付环境缺少 MSVC C++ 工具链，无法在此机器完成最终打包，需在装有 VS Build Tools 的机器上执行 |

> 由于交付机没有 Visual Studio C++ 生成工具，本次用 `x86_64-pc-windows-gnu` 目标 + rust-lld/zig 做了
> 交叉验证（这也是 `tools/` 存在的原因）。正式的 `pnpm tauri:build` 请按上面「开发环境」一节装好依赖后执行。


---

## License

MIT
