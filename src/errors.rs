use std::io;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum ProbeError {
    #[error("sysfs error: {0}")]
    SysfsError(io::Error),
    #[error("procfs error: {0}")]
    ProcfsError(io::Error),
    #[error("invalid cpu ID: {0}")]
    InvalidCpuId(String),
}
