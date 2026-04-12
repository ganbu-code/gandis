pub mod frame;
pub mod connection;
pub mod db;
pub mod handle;
pub mod aof;

pub const DEFAULT_PORT: &str = "6379";
pub const AOF_FILE: &str = "aof-file.aof";
// 是否开启AOF重放,默认开启
pub const AOF_RELOAD: bool = true;
// 是否开启AOF,默认开启 
pub const AOF_ENABLE: bool = true;