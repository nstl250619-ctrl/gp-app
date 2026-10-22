//! 加密资源（安全裁决②：注册表明文 → 加密资源）。
//!
//! GPHub 把整份配置（含明文 KEY、ssh 参数、端点表）存进 HKCU 注册表，同用户可读。
//! GP 改为：同一份「state 等价物」用 AES-256-GCM 加密后作为资源分发，明文只在内存出现。
//!
//! 定位澄清（安全边界诚实声明）：
//!   secrets.enc 只存「公开配置」（host key 公钥、ssh 地址、端点转发表），不含真正机密。
//!   加密用于：①防篡改（GCM 认证 + 魔数校验）②防跨机器复制（KDF 掺入本机因素）。
//!   真正机密（API key、SSH 私钥）一律走系统钥匙串（credential.rs），不在此处。
//!   因此这里的加密是「配置打包格式」，不是抗逆向的机密存储，不对外宣传为安全边界。
//!
//! 打包期（构建脚本，不在本模块运行）：
//!   明文 secrets.json ──AES-256-GCM──▶ secrets.enc
//!   结构：[ magic "GPSE" (4B) ][ nonce (12B) ][ ciphertext ]
//!   密钥 = SHA256(MASTER_SECRET ‖ KDF_SALT ‖ 本机因素)
//! 运行期：load() 读出密文 → 派生密钥 → 解密到内存结构，绝不写回明文。

use std::path::Path;

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{AppError, AppResult};

/// 密文文件魔数，防文件被误替换/乱改。
const SECRETS_MAGIC: &[u8; 4] = b"GPSE";
/// GCM nonce 长度（12 字节，标准）。
const NONCE_LEN: usize = 12;
/// KDF 盐（域分离，非机密）。
const KDF_SALT: &[u8] = b"greenpool.secrets.v1";
/// 应用常量（非密钥）：参与 KDF 的域分离因子。secrets.enc 仅存公开配置，见模块顶部定位澄清。
const MASTER_SECRET: &str = "GP-MASTER-SECRET-SKELETON-CHANGE-ME";

/// 隧道转发表一项（本机端口 → 服务器目标端口）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Forward {
    pub id: String,
    pub name: String,
    pub local_port: u16,
    pub target_port: u16,
    pub enabled: bool,
    /// 应用层探活路径（相对本机端口）。`None` = 该端点不做应用层探活（apiState 恒 `unchecked`）。
    #[serde(default)]
    pub probe_path: Option<String>,
    /// 视为健康的 HTTP 状态码集合。
    #[serde(default = "default_probe_status")]
    pub probe_success_status: Vec<u16>,
}

fn default_probe_status() -> Vec<u16> {
    vec![200]
}

/// 加密资源解密后的内容（等价于 GPHub 注册表里的 state JSON）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Secrets {
    pub ssh_host: String,
    pub ssh_port: u16,
    pub ssh_user: String,
    /// host key 算法名，如 "ssh-ed25519"。
    pub host_key_type: String,
    /// host key 公钥（base64，无 "ssh-ed25519 " 前缀、无注释）。空 = 未配置（隧道拒绝启动）。
    pub host_key_b64: String,
    pub forwards: Vec<Forward>,
}

impl Default for Secrets {
    fn default() -> Self {
        Self {
            // 直链加速未启用（产品决策）：SSH 目标与凭据全部占位，防真实服务器信息进发布包。
            // 启用时填回：ssh_host / ssh_user / host_key_b64（真实值见运维记录），并恢复指纹自证测试。
            ssh_host: "".into(),
            ssh_port: 2222,
            ssh_user: "".into(),
            host_key_type: "ssh-ed25519".into(),
            // 占位：host key 留空 = 隧道拒绝启动（失败关闭，HostKeyMissing）。
            host_key_b64: "".into(),
            // 端点转发表（唯一正确映射，与 GPHub 一致）：
            //   newapi=3000→13000、agent2api=3065→13065、wb2manager=7864→17864
            // 范围收敛（用户决策）：当前只开放 newapi 一个转发；agent2api/wb2manager 暂不开放，
            // 后续开放时把对应 enabled 改 true 即可（探活路径已实测备好）。
            forwards: vec![
                Forward {
                    id: "newapi".into(),
                    name: "NewAPI 网关".into(),
                    local_port: 3000,
                    target_port: 13000,
                    enabled: true,
                    probe_path: Some("/v1/models".into()),
                    probe_success_status: vec![200, 401],
                },
                Forward {
                    id: "agent2api".into(),
                    name: "agent2api 聚合".into(),
                    local_port: 3065,
                    target_port: 13065,
                    enabled: false,
                    probe_path: Some("/v1/models".into()),
                    probe_success_status: vec![200, 401],
                },
                Forward {
                    id: "wb2manager".into(),
                    name: "wb2manager 控制台".into(),
                    local_port: 7864,
                    target_port: 17864,
                    enabled: false,
                    // 实测（2026-10-21）：/v1/models→401、/healthz→200、/api/status→403。用 /healthz（200）。
                    probe_path: Some("/healthz".into()),
                    probe_success_status: vec![200],
                },
            ],
        }
    }
}

/// 派生 AES-256 密钥（32 字节）。掺入本机因素，防「复制密文到另一台机器也能解密」。
/// 注意：这是轻量混淆，非强安全边界（secrets.enc 只存公开配置，见模块顶部定位）。
fn derive_key() -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(MASTER_SECRET.as_bytes());
    h.update(KDF_SALT);
    for var in ["COMPUTERNAME", "USERDOMAIN", "USERNAME"] {
        if let Ok(v) = std::env::var(var) {
            h.update(v.as_bytes());
        }
    }
    let out = h.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(out.as_slice());
    key
}

/// 加密（构建脚本 / 测试用）：返回 [magic][nonce][ciphertext]。
pub fn encrypt_blob(plaintext: &[u8], nonce: [u8; NONCE_LEN]) -> Vec<u8> {
    let key = derive_key();
    let cipher = Aes256Gcm::new_from_slice(&key).expect("aes-256 密钥长度固定 32B");
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce), plaintext)
        .expect("GCM 加密失败");
    let mut blob = Vec::with_capacity(4 + NONCE_LEN + ciphertext.len());
    blob.extend_from_slice(SECRETS_MAGIC);
    blob.extend_from_slice(&nonce);
    blob.extend_from_slice(&ciphertext);
    blob
}

/// 加密 secrets JSON 明文（构建期工具用）：先校验 JSON 合法（反序列化一次），再加密。
/// 与 `decrypt_blob` 共用同一套密钥派生，保证「构建期加密 ↔ 运行期解密」同源。
pub fn encrypt_json(json_str: &str, nonce: [u8; NONCE_LEN]) -> AppResult<Vec<u8>> {
    serde_json::from_str::<Secrets>(json_str).map_err(AppError::from)?;
    Ok(encrypt_blob(json_str.as_bytes(), nonce))
}

/// 解密（运行期）：校验魔数 → 取 nonce → 解密。
pub fn decrypt_blob(blob: &[u8]) -> AppResult<Vec<u8>> {
    if blob.len() < 4 + NONCE_LEN || &blob[..4] != SECRETS_MAGIC {
        return Err(AppError::Other("加密资源损坏或版本不匹配".into()));
    }
    let nonce = &blob[4..4 + NONCE_LEN];
    let ciphertext = &blob[4 + NONCE_LEN..];
    let key = derive_key();
    let cipher = Aes256Gcm::new_from_slice(&key).expect("aes-256 密钥长度固定 32B");
    cipher
        .decrypt(Nonce::from_slice(nonce), ciphertext)
        .map_err(|_| AppError::Other("加密资源解密失败".into()))
}

/// 加载并解密资源。资源缺失时回退默认（host_key_b64 为空 → 隧道失败关闭）。
pub fn load(data_dir: &Path) -> AppResult<Secrets> {
    let path = data_dir.join("secrets.enc");
    if !path.exists() {
        return Ok(Secrets::default());
    }
    let blob = std::fs::read(&path)?;
    let plain = decrypt_blob(&blob)?;
    serde_json::from_slice(&plain).map_err(AppError::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let nonce = [7u8; NONCE_LEN];
        let plain = br#"{"ssh_host":"156.225.28.144"}"#;
        let blob = encrypt_blob(plain, nonce);
        assert_eq!(decrypt_blob(&blob).unwrap(), plain);
    }

    #[test]
    fn tampered_blob_fails() {
        let nonce = [7u8; NONCE_LEN];
        let mut blob = encrypt_blob(b"secret", nonce);
        let last = blob.len() - 1;
        blob[last] ^= 0xff;
        assert!(decrypt_blob(&blob).is_err());
    }

    #[test]
    fn wrong_magic_fails() {
        assert!(decrypt_blob(b"XXXX").is_err());
    }

    #[test]
    fn encrypt_json_roundtrip() {
        let nonce = [7u8; NONCE_LEN];
        let json = r#"{"sshHost":"1.2.3.4","sshPort":2222,"sshUser":"u","hostKeyType":"ssh-ed25519","hostKeyB64":"AAA","forwards":[]}"#;
        let blob = encrypt_json(json, nonce).unwrap();
        let plain = decrypt_blob(&blob).unwrap();
        assert_eq!(plain, json.as_bytes());
    }

    #[test]
    fn encrypt_json_rejects_invalid() {
        let nonce = [7u8; NONCE_LEN];
        assert!(encrypt_json("not json", nonce).is_err());
    }

    #[test]
    fn default_forwards_map_correct_ports() {
        let s = Secrets::default();
        let mut by_id = std::collections::HashMap::new();
        for f in &s.forwards {
            by_id.insert(f.id.clone(), (f.local_port, f.target_port));
        }
        assert_eq!(by_id.get("newapi"), Some(&(3000, 13000)));
        assert_eq!(by_id.get("agent2api"), Some(&(3065, 13065)));
        assert_eq!(by_id.get("wb2manager"), Some(&(7864, 17864)));
    }

    #[test]
    fn all_enabled_forwards_have_probe_path() {
        // 当前范围只开 newapi（用户决策）；已启用的端点均应有有效应用层探活配置。
        let s = Secrets::default();
        let enabled: Vec<_> = s.forwards.iter().filter(|f| f.enabled).collect();
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].id, "newapi");
        for f in enabled {
            assert!(f.probe_path.is_some(), "{} 缺少探活路径", f.id);
            assert!(!f.probe_success_status.is_empty(), "{} 缺少成功状态码", f.id);
        }
    }
}
