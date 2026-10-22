//! 统一错误枚举（§14）。命令层据此映射成 CommandError 信封。

use thiserror::Error;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("网络连不上中转站")]
    Network(#[from] reqwest::Error),

    #[error("文件读写失败：{0}")]
    Io(#[from] std::io::Error),

    #[error("系统钥匙串不可用：{0}")]
    Keyring(#[from] keyring::Error),

    #[error("配置解析失败：{0}")]
    Json(#[from] serde_json::Error),

    #[error("未找到密钥，请先完成兑换")]
    KeyMissing,

    #[error("没找到目标工具的配置")]
    TargetNotFound,

    #[error("该邮箱已注册，请直接登录")]
    EmailTaken,

    #[error("登录已过期，请重新登录")]
    SessionExpired,

    /// 商城返回 401 且 code=need_bind_newapi：需要用户绑定中转站账号（输一次密码）。
    #[error("请绑定中转站账号")]
    NeedBindNewapi,

    /// 已绑定但中转站密码失效：提示重新绑定。
    #[error("中转站账号密码已失效，请重新绑定")]
    NeedRebindNewapi,

    /// 中转站账号被禁用。
    #[error("中转站账号已被禁用，请联系管理员")]
    NewapiBanned,

    #[error("备份原配置失败，已中止写入：{0}")]
    BackupFailed(String),

    #[error("写入校验未通过：{0}")]
    VerifyFailed(String),

    #[error("还原失败：{0}")]
    RollbackFailed(String),

    /// 还原点是 GP 托管态快照（非「配置前原始备份」）：默认阻止，用户二次确认后可 force 还原。
    #[error("该还原点是 GP 配置快照（非配置前原始备份），需确认后才能还原")]
    RollbackBlocked,

    /// 隧道子系统错误（ssh.exe 启动/巡检失败等）。
    #[error("隧道错误：{0}")]
    Tunnel(String),

    /// 缺少 pinned host key：拒绝以 TOFU 方式连接。
    #[error("缺少服务端 host key，无法建立严格校验的隧道")]
    HostKeyMissing,

    /// host key 指纹与编译期常量不符：疑似中间人或服务器被替换。
    #[error("服务端 host key 指纹不匹配，已拒绝连接")]
    HostKeyMismatch,

    /// SSH 私钥未导入钥匙串。
    #[error("尚未导入 SSH 私钥")]
    SshKeyMissing,

    #[error("{0}")]
    Other(String),
}
