# TomatoSSH 云同步服务端

一个极小的同步服务：邮箱 + 密码注册登录（JWT），把客户端加过密的保险库密文存进 SQLite。

- 技术栈：Rust + axum 0.8 + rusqlite（bundled SQLite）+ 手写 HS256 JWT
- 依赖：只需要 Docker；不需要外部数据库
- 端口：`8787`

## 快速开始

```bash
cp .env.example .env
# 把 JWT_SECRET 换成随机值，生产环境必须固定
#   openssl rand -hex 32
docker compose up -d --build
curl http://127.0.0.1:8787/healthz
```

本地直接跑（需 Rust）：

```bash
cargo run
# 默认 data/tomatossh.db
```

## 环境变量

| 变量 | 默认 | 说明 |
|------|------|------|
| `BIND_ADDR` | `0.0.0.0:8787` | 监听地址 |
| `DATABASE_PATH` | `data/tomatossh.db` | SQLite 文件路径 |
| `JWT_SECRET` | 随机临时值 | 令牌签名密钥，**必须固定**，否则重启后需重新登录 |
| `ALLOW_REGISTER` | `true` | 关闭公开注册（部署完成后建议 `false`） |
| `TOKEN_TTL_HOURS` | `720` | 令牌有效期 |
| `MAX_BLOB_BYTES` | `16777216` | 单个保险库密文上限 |
| `RUST_LOG` | `tomatossh_sync=info` | 日志级别 |

## 接口

```
GET  /healthz                       健康检查
POST /api/v1/auth/register          {email,password}      -> {token,email,expires_in}
POST /api/v1/auth/login             {email,password}      -> {token,email,expires_in}
GET  /api/v1/sync/vault             Bearer -> {revision,blob,updated_at}
PUT  /api/v1/sync/vault             {blob,base_revision?,device_id} -> {revision,updated_at}
```

`blob` 是客户端用 PBKDF2(账号密码) + AES-256-GCM 加密后的 base64 密文，
**服务端只做存储，无法解密**。`base_revision` 与服务端当前版本不一致时返回 `409`，
客户端会提示「先拉取再上传」。

## 数据备份

```bash
docker compose exec tomatossh-sync sh -c 'cp /data/tomatossh.db /data/backup-$(date +%F).db'
# 或直接备份宿主机的 tomatossh-data 卷
```

## 反代（HTTPS）

客户端支持 `http://`，但建议用 Nginx / Caddy 套一层 TLS，配置示例见仓库根目录 `README.md`。
