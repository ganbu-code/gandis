use crate::db::Entry;
use crate::{AOF_ENABLE, AOF_FILE, AOF_RELOAD};
use bytes::Bytes;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::fs::{File, OpenOptions};
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use tokio::time::Instant;

pub struct AOF {
    pub file: Mutex<File>,
}

impl AOF {
    pub async fn new(path: &str) -> Self {
        let aof = OpenOptions::new()
            .append(true)
            .create(true)
            .open(path)
            .await
            .unwrap();
        Self {
            file: Mutex::new(aof),
        }
    }

    pub async fn log(&self, command: &str) {
        if AOF_ENABLE {
            let mut file = self.file.lock().await;
            let _ = file.write_all(command.as_bytes()).await;
            let _ = file.flush().await;
        } else {
            println!("AOF功能未开启,跳过日志记录");
        }
    }

    pub async fn replay(&self, db_aof: &Arc<Mutex<HashMap<String, Entry>>>) {
        if AOF_ENABLE && AOF_RELOAD {
            let aof_data = tokio::fs::read(AOF_FILE).await.map_err(|e| e.to_string());
            if let Ok(data) = aof_data {
                if !data.is_empty() {
                    let data_string = String::from_utf8_lossy(&data).to_string();
                    println!("从AOF文件同步数据");
                    // 简单解析AOF文件中的SET命令
                    for line in data_string.lines() {
                        let line = line.trim();
                        if line.is_empty() {
                            continue;
                        }
                        let parts: Vec<&str> = line.split_whitespace().collect();
                        if parts.len() >= 3 && parts[0].to_uppercase() == "SET" {
                            let key = parts[1].to_string();
                            let value = Bytes::from(parts[2].as_bytes().to_vec());
                            let mut ttl: Option<Instant> = None;
                            if parts.len() > 3 && parts[3] == "TTL" {
                                if let Ok(ttl_u64) = parts[4].parse::<u64>() {
                                    ttl = Some(Instant::now() + Duration::from_secs(ttl_u64));
                                }
                            }
                            let entry = Entry {
                                data: value,
                                ttl: ttl,
                            };
                            db_aof.lock().await.insert(key, entry);
                        } else if parts[0] == "DEL" {
                            db_aof.lock().await.remove(&parts[1].to_string());
                        }
                    }
                }
            }
        } else {
            println!("没有开启AOF重放,跳过数据同步");
        }
    }
}
