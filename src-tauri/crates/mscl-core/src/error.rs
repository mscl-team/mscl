//! core 层统一错误类型

use thiserror::Error;

#[derive(Debug, Clone, Error)]
pub enum CoreError {
    #[error("解析失败: {0}")]
    Parse(String),
    #[error("校验失败: {0}")]
    Checksum(String),
    #[error("网络错误: {0}")]
    Network(String),
    #[error("未找到: {0}")]
    NotFound(String),
}

pub type Result<T> = std::result::Result<T, CoreError>;
