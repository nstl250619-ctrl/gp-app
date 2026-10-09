//! 应用共享状态。

pub struct AppState {
    pub data_dir: std::path::PathBuf,
}

impl AppState {
    pub fn new(data_dir: std::path::PathBuf) -> Self {
        Self { data_dir }
    }
}
