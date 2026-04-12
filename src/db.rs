use bytes::Bytes;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::Instant;

// 实际存储的数据结构体，包括数据实体及对应的TTL
#[derive(Debug, Clone)]
pub struct Entry {
    pub data: Bytes,
    pub ttl: Option<Instant>,
}

// 定义类型别名
pub type Db = Arc<Mutex<HashMap<String, Entry>>>;
