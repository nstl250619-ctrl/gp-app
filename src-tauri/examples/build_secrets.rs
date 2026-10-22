//! 构建期加密工具：secrets.json（明文 JSON，camelCase 对应 Secrets 结构）→ secrets.enc（AES-256-GCM 密文）。
//!
//! 用法：
//!   cargo run --example build_secrets -- [secrets.json] [secrets.enc]
//!   默认输入 secrets.json、输出 secrets.enc。
//!
//! 生成后把 secrets.enc 放入 GP 应用数据目录（或打包进 bundle resources），
//! 运行时 services::secrets::load() 自动解密读取。

use std::path::PathBuf;

/// 密码学安全 nonce（uuid v4 内部基于 CSPRNG/getrandom）。AES-GCM 要求同 key 下 nonce 不重复。
fn gen_nonce() -> [u8; 12] {
    let u = uuid::Uuid::new_v4();
    let mut n = [0u8; 12];
    n.copy_from_slice(&u.as_bytes()[..12]);
    n
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let input = args.get(1).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("secrets.json"));
    let output = args.get(2).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("secrets.enc"));

    let json = std::fs::read_to_string(&input).expect("读 secrets.json 失败");
    let blob = greenpool_lib::services::secrets::encrypt_json(&json, gen_nonce())
        .expect("secrets.json 校验或加密失败");

    std::fs::write(&output, &blob).expect("写 secrets.enc 失败");
    println!("已加密 {} -> {}（{} 字节）", input.display(), output.display(), blob.len());
}
