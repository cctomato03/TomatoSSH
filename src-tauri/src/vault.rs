use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::crypto;
use crate::error::{Error, Result};
use crate::models::{now_ms, Group, GroupDraft, Host, HostDraft, Snapshot, VaultData};

/// 参与云同步的载荷（不含本机密钥，连接密码在整包加密之内）
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SyncPayload {
    pub hosts: Vec<Host>,
    pub groups: Vec<Group>,
    pub updated_at: i64,
}

pub struct VaultStore {
    pub data: VaultData,
    dir: PathBuf,
    key: [u8; 32],
}

impl VaultStore {
    pub fn open(dir: impl AsRef<Path>) -> Result<Self> {
        let dir = dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&dir)?;
        let key = load_or_create_key(&dir)?;
        let path = dir.join("vault.bin");

        let mut data = if path.exists() {
            let blob = std::fs::read(&path)?;
            match crypto::decrypt(&key, &blob) {
                Ok(plain) => serde_json::from_slice::<VaultData>(&plain)
                    .map_err(|e| Error::msg(format!("保险库内容解析失败: {e}")))?,
                Err(e) => return Err(Error::msg(format!("打开保险库失败: {e}"))),
            }
        } else {
            VaultData::default()
        };

        if data.secrets.device_id.is_empty() {
            data.secrets.device_id = uuid::Uuid::new_v4().to_string();
        }

        let store = Self { data, dir, key };
        store.save()?;
        Ok(store)
    }

    pub fn save(&self) -> Result<()> {
        let plain = serde_json::to_vec(&self.data)?;
        let blob = crypto::encrypt(&self.key, &plain)?;
        let path = self.dir.join("vault.bin");
        let tmp = self.dir.join("vault.bin.tmp");
        std::fs::write(&tmp, &blob)?;
        if std::fs::rename(&tmp, &path).is_err() {
            // Windows 上目标存在时 rename 可能失败，退化为直接覆盖
            std::fs::write(&path, &blob)?;
            let _ = std::fs::remove_file(&tmp);
        }
        Ok(())
    }

    pub fn snapshot(&self) -> Snapshot {
        let mut hosts: Vec<Host> = self.data.hosts.iter().filter(|h| !h.deleted).cloned().collect();
        hosts.sort_by_key(|h| h.label.to_lowercase());
        let mut groups: Vec<Group> = self.data.groups.iter().filter(|g| !g.deleted).cloned().collect();
        groups.sort_by_key(|g| (g.sort_order, g.name.to_lowercase()));
        Snapshot {
            hosts,
            groups,
            settings: self.data.settings.clone(),
        }
    }

    pub fn host(&self, id: &str) -> Result<Host> {
        self.data
            .hosts
            .iter()
            .find(|h| h.id == id && !h.deleted)
            .cloned()
            .ok_or_else(|| Error::msg(format!("找不到连接 {id}")))
    }

    pub fn upsert_host(&mut self, draft: HostDraft) -> Result<Host> {
        if draft.host.trim().is_empty() {
            return Err(Error::msg("主机地址不能为空"));
        }
        if draft.username.trim().is_empty() {
            return Err(Error::msg("用户名不能为空"));
        }
        let now = now_ms();
        let existing_idx = draft
            .id
            .as_ref()
            .and_then(|id| self.data.hosts.iter().position(|h| &h.id == id));

        let host = match existing_idx {
            Some(idx) => {
                let prev = self.data.hosts[idx].clone();
                let host = Host {
                    id: prev.id,
                    group_id: draft.group_id,
                    label: draft.label,
                    host: draft.host.trim().to_string(),
                    port: if draft.port == 0 { 22 } else { draft.port },
                    username: draft.username,
                    auth: draft.auth,
                    color: draft.color,
                    tags: draft.tags,
                    notes: draft.notes,
                    keepalive_secs: draft.keepalive_secs.max(5),
                    host_key_fingerprint: prev.host_key_fingerprint,
                    updated_at: now,
                    deleted: false,
                };
                self.data.hosts[idx] = host.clone();
                host
            }
            None => {
                let host = Host {
                    id: uuid::Uuid::new_v4().to_string(),
                    group_id: draft.group_id,
                    label: draft.label,
                    host: draft.host.trim().to_string(),
                    port: if draft.port == 0 { 22 } else { draft.port },
                    username: draft.username,
                    auth: draft.auth,
                    color: draft.color,
                    tags: draft.tags,
                    notes: draft.notes,
                    keepalive_secs: draft.keepalive_secs.max(5),
                    host_key_fingerprint: None,
                    updated_at: now,
                    deleted: false,
                };
                self.data.hosts.push(host.clone());
                host
            }
        };
        self.save()?;
        Ok(host)
    }

    pub fn delete_host(&mut self, id: &str) -> Result<()> {
        let now = now_ms();
        for h in self.data.hosts.iter_mut() {
            if h.id == id {
                h.deleted = true;
                h.updated_at = now;
            }
        }
        self.save()
    }

    pub fn upsert_group(&mut self, draft: GroupDraft) -> Result<Group> {
        if draft.name.trim().is_empty() {
            return Err(Error::msg("分组名称不能为空"));
        }
        let now = now_ms();
        let existing_idx = draft
            .id
            .as_ref()
            .and_then(|id| self.data.groups.iter().position(|g| &g.id == id));

        let group = match existing_idx {
            Some(idx) => {
                let prev = self.data.groups[idx].clone();
                let group = Group {
                    id: prev.id,
                    name: draft.name.trim().to_string(),
                    parent_id: draft.parent_id,
                    color: draft.color,
                    sort_order: draft.sort_order.unwrap_or(prev.sort_order),
                    updated_at: now,
                    deleted: false,
                };
                self.data.groups[idx] = group.clone();
                group
            }
            None => {
                let next_order = self.data.groups.iter().map(|g| g.sort_order).max().unwrap_or(0) + 1;
                let group = Group {
                    id: uuid::Uuid::new_v4().to_string(),
                    name: draft.name.trim().to_string(),
                    parent_id: draft.parent_id,
                    color: draft.color,
                    sort_order: draft.sort_order.unwrap_or(next_order),
                    updated_at: now,
                    deleted: false,
                };
                self.data.groups.push(group.clone());
                group
            }
        };
        self.save()?;
        Ok(group)
    }

    pub fn delete_group(&mut self, id: &str) -> Result<()> {
        let now = now_ms();
        // 子分组与连接上移到根，避免数据“消失”
        for g in self.data.groups.iter_mut() {
            if g.id == id {
                g.deleted = true;
                g.updated_at = now;
            } else if g.parent_id.as_deref() == Some(id) {
                g.parent_id = None;
                g.updated_at = now;
            }
        }
        for h in self.data.hosts.iter_mut() {
            if h.group_id.as_deref() == Some(id) {
                h.group_id = None;
                h.updated_at = now;
            }
        }
        self.save()
    }

    pub fn set_settings(&mut self, settings: crate::models::Settings) -> Result<()> {
        self.data.settings = settings;
        self.save()
    }

    pub fn set_host_fingerprint(&mut self, id: &str, fingerprint: &str) -> Result<()> {
        for h in self.data.hosts.iter_mut() {
            if h.id == id {
                h.host_key_fingerprint = Some(fingerprint.to_string());
            }
        }
        self.save()
    }

    pub fn clear_host_fingerprint(&mut self, id: &str) -> Result<()> {
        for h in self.data.hosts.iter_mut() {
            if h.id == id {
                h.host_key_fingerprint = None;
            }
        }
        self.save()
    }

    pub fn payload(&self) -> SyncPayload {
        SyncPayload {
            hosts: self.data.hosts.clone(),
            groups: self.data.groups.clone(),
            updated_at: now_ms(),
        }
    }

    /// 把云端数据合入本地（按 updatedAt 后写者胜），返回 (新增/更新条数)
    pub fn merge_payload(&mut self, remote: SyncPayload) -> Result<usize> {
        let mut changed = 0usize;

        for rh in remote.hosts {
            match self.data.hosts.iter_mut().find(|h| h.id == rh.id) {
                Some(local) => {
                    if rh.updated_at > local.updated_at {
                        *local = rh;
                        changed += 1;
                    }
                }
                None => {
                    self.data.hosts.push(rh);
                    changed += 1;
                }
            }
        }

        for rg in remote.groups {
            match self.data.groups.iter_mut().find(|g| g.id == rg.id) {
                Some(local) => {
                    if rg.updated_at > local.updated_at {
                        *local = rg;
                        changed += 1;
                    }
                }
                None => {
                    self.data.groups.push(rg);
                    changed += 1;
                }
            }
        }

        if changed > 0 {
            self.save()?;
        }
        Ok(changed)
    }
}

fn load_or_create_key(dir: &Path) -> Result<[u8; 32]> {
    let path = dir.join("vault.key");
    if path.exists() {
        let raw = std::fs::read(&path)?;
        if raw.len() == 32 {
            let mut key = [0u8; 32];
            key.copy_from_slice(&raw);
            return Ok(key);
        }
        return Err(Error::msg("保险库密钥文件长度异常，已拒绝加载"));
    }
    let key_vec = crypto::random_bytes(32);
    let mut key = [0u8; 32];
    key.copy_from_slice(&key_vec);
    std::fs::write(&path, key)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(key)
}
