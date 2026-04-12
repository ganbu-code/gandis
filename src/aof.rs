use crate::db::Entry;
use crate::frame::Frame;
use crate::handle::apply_storage_command;
use crate::{AOF_ENABLE, AOF_FILE, AOF_RELOAD};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::fs::{File, OpenOptions};
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

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
                    for line in data_string.lines() {
                        let line = line.trim();
                        if line.is_empty() {
                            continue;
                        }
                        let frames: Vec<Frame> = line
                            .split_whitespace()
                            .map(|token| Frame::Bulk(token.to_string().into()))
                            .collect();
                        if let Err(err) = apply_storage_command(&frames, db_aof).await {
                            println!("AOF重放跳过命令 [{}], 原因: {}", line, err);
                        }
                    }
                }
            }
        } else {
            println!("没有开启AOF重放,跳过数据同步");
        }
    }
}
