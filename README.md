# 🍅 TomatoSSH

一个模仿 Termius 的跨平台 SSH / SFTP 桌面客户端，支持 **Windows / macOS / Ubuntu Desktop**。
连接信息保存在本地加密保险库中，也可以部署一个自己的同步服务（Docker 一键起）在多台电脑之间同步。

> 技术栈：**Tauri 2（Rust）+ React 19 + TypeScript + xterm.js**，后端 SSH 使用纯 Rust 的 `russh`，
> 本地终端使用 `portable-pty`，云同步服务端为 `axum + SQLite`（Rust），全部打包成单个二进制。

---

## 功能一览

| # | 需求 | 实现情况 |
|---|------|----------|
| 1 | 连接信息管理与本地持久化，密码 / 密钥登录 | ✅ 连接 + 多级分组管理，数据写入 `vault.bin`（AES-256-GCM 加密）；支持密码认证、私钥认证（含 passphrase）；**支持跳板机 ProxyJump** |
| 2 | 远程文件上传下载 + 不同远程主机之间互传 | ✅ **独立的文件管理器窗口**（不再挤在终端右侧）：左右两侧各自可选「本地磁盘」或任意一台远程主机，因此 本地 ⇄ 远程、**远程 ⇄ 远程**（服务器之间直传，不经过本机磁盘）都支持；文件夹递归传输、4 路并发、保留时间戳/权限；传输走**后台队列**（断点续传 + 失败自动重试），关掉窗口也会继续传 |
| 3 | 分组 + 左侧连接列表 + 右侧终端，多连接并行 | ✅ 左侧为分组树 / 连接列表 / 搜索，右侧为多标签终端，标签可并行打开多个会话 |
| 4 | 字号调整 + 主题切换（内置多套美观主题） | ✅ 设置面板可调终端字号、行高、字体、光标闪烁、回滚缓冲，以及 **界面文字 85%~140% 全局缩放**（`--ui-scale` CSS 变量，终端字号独立）；内置 **8 套主题**（番茄暗夜 / Nord / Dracula / Gruvbox / One Dark / 东京夜色 / 暖阳浅色 / 素白 GitHub） |
| 5 | 支持操作系统本地终端 | ✅ 侧边栏「本地终端」或在标签栏 `+` 新建，Windows 自动选 `pwsh`/`powershell`/`cmd`，Linux/macOS 使用 `$SHELL` |
| 7 | 串口连接（终端交互） | ✅ 串口配置管理（端口 / 波特率 / 数据位 / 校验 / 停止位 / 流控）+ 终端交互；端口自动枚举与**手动输入**；**串口不提供文件传输**；另有 **DTR/RTS 手动控制 + 一键复位脉冲**、**USB 掉线自动重连**、**HEX 十六进制收发视图 + 发送栏**、**自动探测波特率**、**端口自动找回**、**日志录制（按天分文件，可选原始字节或带时间戳文本）** |
| 8 | 快捷指令 | ✅ 全局快捷指令库（SSH / 本地终端 / 串口通用）：终端上方点一下即把命令写进终端，可设"自动回车执行"，`Shift+点击` 只粘贴不执行；**支持分组归类 + 搜索/按分组过滤**；指令存加密保险库并随云同步 |
| 9 | 端口转发 | ✅ 本地转发（等价 `ssh -L`）：本机端口 → 经 SSH 隧道到远端目标；规则按连接保存、可随时启停、可设"开终端时自动启动"，列表里能看到已建立的连接数 |
| 6 | 连接信息云同步（自建 Docker 服务） | ✅ 自建账号体系（邮箱 + 密码，JWT），`server/` 一键 Docker 部署；**客户端先加密再上传，服务端看不到明文**，多设备按修改时间自动合并 |

其他细节：

- 主机公钥指纹 TOFU 校验（首次连接记录指纹，指纹变化会中断连接并提示，可在连接编辑里清除）
- 传输进度条、面板错误提示、右键菜单（连接 / 编辑 / 复制 / 删除 / 新建子分组…）
- 终端快捷键：`Ctrl/Cmd + Shift + C` 复制、`Ctrl/Cmd + Shift + V` 粘贴、`Ctrl/Cmd + T` 新建本地终端、`Ctrl/Cmd + Shift + F` 打开文件管理器窗口、`Ctrl/Cmd + Shift + W` 关闭标签
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
| Ubuntu / Debian | `sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev libudev-dev pkg-config`（`libudev-dev` 是串口枚举需要的） |

### 本地开发

```bash
pnpm install
pnpm tauri:dev        # 打开开发窗口（前端热更新）
```

### 打包

```bash
pnpm tauri:build                 # 打当前系统全部格式
pnpm tauri:build --bundles msi   # 只打 Windows MSI
pnpm tauri:build --bundles nsis  # 只打 Windows NSIS 安装程序
pnpm tauri:build --bundles deb   # 只打 Ubuntu deb
pnpm tauri:build --bundles dmg   # 只打 macOS dmg
```

产物在 `src-tauri/target/release/bundle/`：

- Windows：`.msi`（WiX）/ `-setup.exe`（NSIS）
- macOS：`.dmg` / `.app`
- Linux：`.deb` / `.AppImage`

> 纯前端调试可以用 `pnpm dev`（但 `invoke` 调用需要 Tauri 运行时，浏览器里会提示）。
> 本次已打好的 Windows 安装包已复制到仓库根目录 `release/`（不入 git）：
> `TomatoSSH_1.1.0_x64_en-US.msi`、`TomatoSSH_1.1.0_x64-setup.exe`、`TomatoSSH-1.1.0-x64-portable.zip`。

#### ⚠️ 国内网络下打包 Windows 安装包的坑

Tauri 打包 MSI/NSIS 时会从 GitHub 下载 WiX 与 NSIS 工具链，多数国内网络会超时
（报错 `failed to bundle project: timeout: global`）。两个办法：

1. **预置工具链（已验证可用）**：把 WiX 解压到 Tauri 的缓存目录，打包时就不会再下载。

   ```powershell
   $dest = "$env:LOCALAPPDATA\tauri\WixTools314"
   New-Item -ItemType Directory -Force $dest | Out-Null
   # 用镜像下载（gh-proxy.com / ghfast.top 均可）
   curl.exe -L -o "$env:TEMP\wix.zip" `
     "https://gh-proxy.com/https://github.com/wixtoolset/wix3/releases/download/wix3141rtm/wix314-binaries.zip"
   Expand-Archive "$env:TEMP\wix.zip" -DestinationPath $dest -Force
   test-path "$dest\candle.exe"   # 必须为 True
   pnpm tauri:build --bundles msi
   ```

2. 或直接用 GitHub Actions（`.github/workflows/build.yml`）在 CI 上出包，见下节。

### 跨平台打包：必须"在哪个系统出哪个包"

| 目标平台 | 能否在 Windows 上打 | 说明 |
|---|---|---|
| Windows `.msi` / `-setup.exe` | ✅ 本机即可 | 需要 VS Build Tools（C++ 生成工具）+ WebView2 Runtime |
| macOS `.dmg` / `.app` | ❌ **不能** | 必须在 macOS 上打（依赖 Xcode / clang / SDK），且对外分发还需要 Apple Developer 账号做 codesign + notarization，否则用户首次打开会被 Gatekeeper 拦 |
| Ubuntu `.deb` / `.AppImage` | ❌ 不建议 | 官方不支持从 Windows 交叉编译；需要 Linux 环境（Ubuntu 实体机 / 虚拟机 / WSL2）并装 `libwebkit2gtk-4.1-dev` 等依赖 |

所以最省事的做法是**用仓库里已备好的 GitHub Actions 矩阵**（`.github/workflows/build.yml`）：
推一个 tag（如 `git tag v0.1.0 && git push --tags`）或手动触发，CI 会分别在
`windows-latest`、`macos-latest`（x64 + Apple Silicon）、`ubuntu-22.04` 上出包并上传到 Release。

若没有 GitHub，也可以：Windows 本机出 Windows 包、Ubuntu 机器/WSL2 出 Linux 包、借一台 Mac（或用云 Mac 按小时租）出 macOS 包。

#### CI 常见失败（已在仓库内规避）

| 症状 | 原因 | 处理 |
|---|---|---|
| 4 个 job 都在 `pnpm/action-setup` 步骤 **1 秒内失败**，后续全 `skipped` | `package.json` 里的 `packageManager` 已声明 pnpm 版本，workflow 又写了 `with: version`，action 报 `Multiple versions of pnpm specified` | workflow 里只保留 `pnpm/action-setup@v4` + `run_install: false`，版本统一由 `packageManager` 决定 |
| 编译都跑完了，最后一步报 `Error: Resource not accessible by integration`（`create-a-release`） | `GITHUB_TOKEN` 默认只读，工作流没有声明写权限 | workflow 顶部加 `permissions: { contents: write }`；若仍失败，去 Settings → Actions → General → Workflow permissions 选 "Read and write permissions" |
| `unable to parse JSON Tauri config file ... expected value at line 1 column 1` | `tauri.conf.json` 带了 **UTF-8 BOM**（Windows 上用 PowerShell `Set-Content -Encoding UTF8` 会写入 BOM） | 用无 BOM 的 UTF-8 保存；本仓库已清理 |
| Linux 打包缺 `webkit2gtk` / appindicator | 依赖名在 22.04 应为 `libwebkit2gtk-4.1-dev`、`libayatana-appindicator3-dev` | 见 workflow 的「安装 Linux 打包依赖」步骤 |
| tag 是 `v1.0.1` 但安装包叫 `1.0.0` | `tauri.conf.json` 的 `version` 与 tag 不一致 | 现在由 workflow 的「版本号对齐 git tag」步骤自动同步（`node scripts/set-version.mjs`）；本地发版手动跑一次同样命令即可 |


### 版本号：跟 git tag 保持一致

App / 安装包里的版本号**只认一个来源**：`src-tauri/tauri.conf.json` 的 `version`
（Windows 的 `FileVersion`/`ProductVersion`、MSI/NSIS 安装包文件名、macOS 的 CFBundleShortVersionString 都由它生成；
`package.json`、`src-tauri/Cargo.toml`、`Cargo.lock` 需要与它一致）。

CI 里由 tag 自动驱动，不需要手动改版本号（`.github/workflows/build.yml` → 「版本号对齐 git tag」步骤）：

```yaml
VERSION="${GITHUB_REF_NAME#v}"      # tag v1.2.1 → 1.2.1
node scripts/set-version.mjs "$VERSION"
```

**预发布版本（RC）注意**：Windows 的 MSI(WiX) 要求预发布标识必须是**纯数字且 ≤65535**，`1.0.0-rc` 这种会直接报
`optional pre-release identifier in app version must be numeric-only ...`。所以默认策略是：

```bash
node scripts/set-version.mjs v1.0.0-rc    # → 应用版本 1.0.0（tag 仍叫 v1.0.0-rc，Release 名也带 rc）
node scripts/set-version.mjs v1.0.0-rc --keep-pre  # → 应用版本 1.0.0-rc（只能出 NSIS/AppImage/dmg，不能出 MSI）
```

CI 的「版本号对齐 git tag」步骤走的是默认策略，所以推 `v1.0.0-rc` 也能四平台全绿，安装包文件名会是 `TomatoSSH_1.0.0_x64_...`，
而 Release 标题/标签仍是 `v1.0.0-rc`。

本地发版也可用同一个脚本一次改四处：

```bash
node scripts/set-version.mjs 1.2.2
python scripts/print-version.py     # 核对四个文件是否一致
```

想确认已装版本：设置面板底部会显示 `TomatoSSH vX.Y.Z`（读自 `CARGO_PKG_VERSION`，与上面同步）；
也可以在 Windows 上右键 exe → 属性 → 详细信息查看「文件版本」。



### 常用脚本

```bash
pnpm typecheck                     # 前端类型检查
pnpm build                         # 类型检查 + 生产构建
cd src-tauri && cargo check        # 后端编译检查
cd server && cargo run             # 本地启动同步服务（默认 0.0.0.0:8787）
node scripts/set-version.mjs 1.2.2 # 同步四处版本号（含 tauri.conf.json）
python scripts/print-version.py    # 打印四处版本号核对
```

---

## 使用说明

1. **新建连接**：左下角「连接」或侧边栏右键 → 填主机 / 端口 / 用户名，选择密码或私钥（可点「选择…」挑私钥文件）。
2. **分组**：左下角「分组」，或右键分组 →「新建子分组」；连接和分组都支持右键重命名 / 删除 / 复制。
3. **打开终端**：双击连接或点行尾 ▶。同一个连接重复打开会复用已存在的标签页。
   - **断线重连**：连接断开 / 登录失败 / 本地 shell 退出后，终端底部会出现「重新连接」按钮，点一下即用同一标签、同一主机重新连，原终端内容保留并在末尾追加一条分隔提示（串口标签同理，可手动重开）。
4. **文件传输**：在 SSH 标签页点右上角 📂 图标（或侧边栏右键连接 →「在文件管理器中打开」），打开**独立的文件管理器窗口**：
   - 左右两个面板各自有下拉框：可选「💻 本地磁盘」或任意一台远程主机；
   - 两边都是远程时中间按钮即**服务器之间直传**（数据不经过本机磁盘）；
   - 中间 `→` / `←` 把选中项复制到另一侧，支持**文件夹**（递归）、多选（`Ctrl/Cmd` 点选）、新建文件夹、重命名、删除（文件夹递归删除）；
   - 底部是**后台传输队列**：单目录内 4 路并发，先显示「扫描文件夹…」再传，进度按字节汇总；上传/下载会**保留原文件的修改时间与权限**；
   - 点中间按钮只是**把任务加入队列**，真正的传输跑在 Rust 侧后台任务里 —— **关掉文件管理窗口也不会中断**；
   - 队列持久化到 `transfer-queue.json`，**App 重启后未完成的任务会自动重新排队并断点续传**；
   - 每个文件失败自动重试 3 次（指数退避），重试时**从断点续传**而非从头开始；整个任务失败还会重排 1 次；队列里可随时「取消 / 重试 / 清理已完成」；
   - 成功的任务会在列表里**停留 4 秒后自动消失**（失败/取消的会保留，方便重试）；「清理已完成」以 Rust 侧队列为准，不会残留幽灵条目。
5. **本地终端**：侧边栏「本地终端」或标签栏 `+`。
6. **设置**：右上角齿轮 → 主题 / **界面缩放（85%~140%，只影响界面文字，不影响终端字体）** / 终端字体 / 字号 / 行高 / 回滚缓冲 / 光标闪烁 / 串口日志目录与格式，改动立即生效。
7. **云同步**：右上角云图标 → 填服务器地址 + 邮箱 + 密码 → 注册或登录 → 「立即双向同步」。
8. **串口终端**：侧边栏底部 🔌 按钮（或列表里串口项的右键菜单）→ 新建串口连接：
   - 自动枚举本机串口（Windows `COM3` / Linux `/dev/ttyUSB0` / macOS `/dev/cu.usbserial-*`），列表为空时可手动输入设备路径；
   - 参数：波特率（常用值下拉 + 自定义）、数据位 5~8、校验 none/odd/even、停止位 1/2、流控 无/软件(XON/XOFF)/硬件(RTS/CTS)；
   - 双击串口项即打开独立终端标签，收发数据直接进 xterm；**串口只做终端交互，不提供文件传输**；
   - 配置同样存在加密保险库里（无密码字段），并会随云同步一起同步；
   - 串口标签上方有工具栏：**DTR / RTS 手动电平切换**（CTS/CD 只读指示）、**一键复位脉冲**（DTR/RTS 拉低再释放，调试开发板常用）、**HEX 视图**（十六进制 dump：偏移 + HEX + ASCII，可清空 / 自动滚动）；
   - 工具栏右侧是发送栏：文本或 **HEX 输入**（如 `AA 55 0D`）、换行选择（无 / CRLF / LF / CR）、最近发送历史（10 条）；
   - 工具栏「录制」把收到的数据存到设置里的日志目录，**按天分文件**（`<端口>-YYYY-MM-DD.log`），格式可选 **原始字节** 或 **带时间戳文本**（每行 `[日期 时间]`，控制字符转义为 `\\xNN`）；
   - 配置里可勾选**掉线后自动重连**：USB 被拔/设备重启后每 1.5 秒重试并自动恢复会话（终端里会打印重连提示）。

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

- SSH 支持**密码 / 私钥**认证、**跳板机 ProxyJump**、**本地端口转发（-L）**；尚未实现 `ssh-agent` 转发、远程转发（-R）、动态 SOCKS（-D）与多级跳板链。
- SFTP 已支持**文件夹递归上传 / 下载 / 远程互传 / 递归删除**、单目录 4 路并发、**断点续传 + 失败自动重试 + 后台传输队列（持久化）**，并保留文件修改时间/权限（**目录自身**的 mtime 不保留）。
- 文件管理器是独立窗口，但**关闭主窗口会一起退出**（暂不支持"关掉终端只留文件窗口"）。
- 串口：只做终端交互（无文件传输）；已支持 DTR/RTS、HEX 视图、日志录制（按天分文件）；尚未支持 **自动波特率续传/协议解析**、**按大小切分日志**、**远程转发 -R / SOCKS -D**。
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
| 单元测试（`cargo test --lib`） | ✅ **17 项全过**：串口参数解析/枚举/错误路径/控制线序列化；端口转发参数校验；SFTP 路径与目录扫描；`forward::e2e` 与 `ssh::e2e` 见下 |
| **ProxyJump 端到端**（真实 OpenSSH：跳板机 2222 → 内网 sshd 2223） | ✅ `ssh::e2e::connects_through_jump_host`：先断言"内网目标直连必然失败"，再经跳板机建立连接并**在隧道里执行 `echo` 命令拿到回显**，且保留了跳板机连接 |
| 串口日志单元测试 | ✅ 3 项：格式解析与文件名（`/dev/ttyUSB0` → 安全文件名）、控制字符转义（`\\x00`/`\\x01` 保留 Tab）、**按天轮转**（跨天新文件 + 换日说明 + 未换行残片先落旧文件）与 **raw 模式字节完全不变** |
| **本地端口转发端到端** | ✅ `forward::e2e::local_forward_reaches_inner_service`：启动 `-L` 后连本机随机端口，**透过隧道读到内网 sshd 的 `SSH-2.0` 横幅**，停止后列表清空 |
| 同步服务端编译 | ✅ 通过 |
| 同步服务端运行 + 接口联调（注册 / 登录 / 密码错误 / 上传 / 拉取 / 版本冲突 409 / 重复注册 409） | ✅ 全部符合预期 |
| Windows Release 编译（`rustc 1.99.1` + MSVC 14.44 + Windows SDK 10.0.26100） | ✅ 通过（6.5 MB 单文件 exe） |
| SFTP 端到端测试（真实 OpenSSH 服务器，docker 起） | ✅ `cargo test --lib sftp` 5 项全过：23 个文件的多层目录递归上传 / 下载 / 远程互传 / 递归删除；校验**上传与下载后 mtime 均被保留**、并发传输正确、进度事件含"扫描中"与完成态；另含**下载断点续传**（本地截断一半后重下内容完整无重复）与**上传断点续传**（远端半截 3000B → 续传到 5000B 且不截断） |
| 独立文件管理器窗口 | ✅ 实测按 `Ctrl+Shift+F` 后进程内出现第二个窗口（标题 `TomatoSSH 文件管理`），与主窗口并存 |
| 桌面应用实际启动 | ✅ 启动后窗口标题 `TomatoSSH`、内存 ~23 MB，并在 `%APPDATA%\com.tomatossh.desktop\` 生成加密保险库 `vault.bin`(450B) + `vault.key`(32B)，文件头为 `TSSH1` 魔数、正文为密文 |
| Windows 安装包 | ✅ `TomatoSSH_1.1.0_x64_en-US.msi`(3.3 MB)、`TomatoSSH_1.1.0_x64-setup.exe`(2.5 MB)、便携版 zip，均在本地 MSVC 工具链下打包并实测可启动 |
| macOS / Linux 安装包 | ⏳ 需在对应系统或 CI 上构建（见「跨平台打包」一节） |

> 由于交付机最初没有 Visual Studio C++ 生成工具，开发期先用 `x86_64-pc-windows-gnu` 目标 + rust-lld/zig 做了
> 交叉验证（这也是 `tools/` 存在的原因）；装好 VS Build Tools 后已用官方 MSVC 工具链完成上面表格中的 Release 构建与安装包打包。


---

## License

MIT
