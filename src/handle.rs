use crate::aof::AOF;
use crate::connection::Connection;
use crate::db::Entry;
use crate::frame::Frame;
use bytes::Bytes;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::Instant;

pub async fn apply_storage_command(
    element: &Vec<Frame>,
    db_clone: &Arc<Mutex<HashMap<String, Entry>>>,
) -> Result<(), String> {
    if let Some(Frame::Bulk(command)) = element.get(0) {
        let cmd = String::from_utf8_lossy(command).to_uppercase();
        match cmd.as_str() {
            "SET" => {
                if let (Some(Frame::Bulk(key)), Some(Frame::Bulk(value))) =
                    (element.get(1), element.get(2))
                {
                    let k = String::from_utf8_lossy(key).to_string();
                    let mut ttl: Option<Instant> = None;
                    if let (Some(Frame::Bulk(ttl_key)), Some(Frame::Bulk(ttl_time_bytes))) =
                        (element.get(3), element.get(4))
                    {
                        if String::from_utf8_lossy(ttl_key).to_string() == "TTL" {
                            let ttl_time_result = String::from_utf8_lossy(ttl_time_bytes)
                                .to_string()
                                .parse::<u64>();
                            if let Ok(ttl_time_u64) = ttl_time_result {
                                ttl = Some(Instant::now() + Duration::from_secs(ttl_time_u64));
                            }
                        }
                    }
                    let entry = Entry {
                        data: value.clone(),
                        ttl,
                    };
                    db_clone.lock().await.insert(k, entry);
                    Ok(())
                } else {
                    Err("SET missing key/value".to_string())
                }
            }
            "SETNX" => {
                if let (Some(Frame::Bulk(key)), Some(Frame::Bulk(value))) =
                    (element.get(1), element.get(2))
                {
                    let k = String::from_utf8_lossy(key).to_string();
                    let mut db_lock = db_clone.lock().await;
                    if db_lock.get(&k).is_none() {
                        let entry = Entry {
                            data: value.clone(),
                            ttl: None,
                        };
                        db_lock.insert(k, entry);
                    }
                    Ok(())
                } else {
                    Err("SETNX missing key/value".to_string())
                }
            }
            "SETXX" => {
                if let (Some(Frame::Bulk(key)), Some(Frame::Bulk(value))) =
                    (element.get(1), element.get(2))
                {
                    let k = String::from_utf8_lossy(key).to_string();
                    let mut db_lock = db_clone.lock().await;
                    if db_lock.get(&k).is_some() {
                        let entry = Entry {
                            data: value.clone(),
                            ttl: None,
                        };
                        db_lock.insert(k, entry);
                    }
                    Ok(())
                } else {
                    Err("SETXX missing key/value".to_string())
                }
            }
            "DEL" => {
                if let Some(Frame::Bulk(key)) = element.get(1) {
                    let key = String::from_utf8_lossy(key).to_string();
                    db_clone.lock().await.remove(&key);
                    Ok(())
                } else {
                    Err("DEL missing key".to_string())
                }
            }
            "EXPIRE" => {
                if let (Some(Frame::Bulk(key_element)), Some(Frame::Bulk(ttl_element))) =
                    (element.get(1), element.get(2))
                {
                    let key = String::from_utf8_lossy(key_element).to_string();
                    let ttl_result = String::from_utf8_lossy(ttl_element).parse::<u64>();
                    if let Ok(ttl) = ttl_result {
                        let mut db_lock = db_clone.lock().await;
                        if let Some(entry) = db_lock.get(&key) {
                            let new_entry = Entry {
                                data: entry.data.clone(),
                                ttl: Some(Instant::now() + Duration::from_secs(ttl)),
                            };
                            db_lock.insert(key, new_entry);
                        }
                        Ok(())
                    } else {
                        Err("EXPIRE ttl parse error".to_string())
                    }
                } else {
                    Err("EXPIRE missing args".to_string())
                }
            }
            _ => Err(format!("unsupported replay command: {}", cmd)),
        }
    } else {
        Err("invalid command frame".to_string())
    }
}

pub async fn ping(connection: &mut Connection) {
    connection
        .write_frame(&Frame::Simple("PONG".to_string()))
        .await
        .unwrap();
}

pub async fn del(
    connection: &mut Connection,
    element: &Vec<Frame>,
    db_clone: &Arc<Mutex<HashMap<String, Entry>>>,
    aof_file: &Arc<AOF>,
) {
    if let Some(Frame::Bulk(key)) = element.get(1) {
        let key = String::from_utf8_lossy(&key).to_string();
        // 只锁一次避免死锁,如果在if上lock,则锁会一直保持,直到if块返回,if里面就不能lock操作
        let mut db_lock = db_clone.lock().await;
        if db_lock.contains_key(&key) {
            db_lock.remove(&key);
            // 删除锁
            drop(db_lock);
            connection.write_frame(&Frame::Integer(1)).await.unwrap();
        } else {
            connection
                .write_frame(&Frame::Simple("None".to_string()))
                .await
                .unwrap();
        }
        // 追加进aof文件
        let cmd = format!("DEL {}\r\n", key);
        aof_file.log(&cmd).await;
    }
}

pub async fn get(
    connection: &mut Connection,
    element: &Vec<Frame>,
    db_clone: &Arc<Mutex<HashMap<String, Entry>>>,
) {
    if let Some(Frame::Bulk(key)) = element.get(1) {
        let key = String::from_utf8_lossy(key).to_string();
        let result = db_clone.lock().await.get(&key).cloned();
        if let Some(value) = result {
            let is_exprired = if let Some(ttl) = value.ttl {
                Instant::now() > ttl
            } else {
                false
            };
            if !is_exprired {
                connection
                    .write_frame(&Frame::Bulk(value.data))
                    .await
                    .unwrap();
            } else {
                db_clone.lock().await.remove(&key);
                connection.write_frame(&Frame::Null).await.unwrap();
            }
        } else {
            connection.write_frame(&Frame::Null).await.unwrap();
        }
    }
}

pub async fn set(
    connection: &mut Connection,
    element: &Vec<Frame>,
    db_clone: &Arc<Mutex<HashMap<String, Entry>>>,
    aof_file: &Arc<AOF>,
) {
    if let (Some(Frame::Bulk(key)), Some(Frame::Bulk(value))) = (element.get(1), element.get(2)) {
        let k = String::from_utf8_lossy(key);
        // 如果数组没有34索引位置的过期时间默认为None
        let mut ttl: Option<Instant> = None;
        // 获取过期时间
        if let (Some(Frame::Bulk(ttl_key)), Some(Frame::Bulk(ttl_time_bytes))) =
            (element.get(3), element.get(4))
        {
            if String::from_utf8_lossy(ttl_key).to_string() == "TTL" {
                let ttl_time_result = String::from_utf8_lossy(ttl_time_bytes)
                    .to_string()
                    .parse::<u64>();
                if let Ok(ttl_time_u64) = ttl_time_result {
                    ttl = Some(Instant::now() + Duration::from_secs(ttl_time_u64));
                }
            }
            let aof_cmd = format!(
                "SET {} {} TTL {}\r\n",
                k,
                String::from_utf8_lossy(value),
                String::from_utf8_lossy(ttl_time_bytes)
            );
            aof_file.log(&aof_cmd).await;
        } else {
            let aof_cmd = format!("SET {} {}\r\n", k, String::from_utf8_lossy(value));
            aof_file.log(&aof_cmd).await;
        }
        let entry = Entry {
            data: value.clone(),
            ttl: ttl,
        };
        db_clone.lock().await.insert(k.to_string(), entry);
        connection
            .write_frame(&Frame::Simple("OK".to_string()))
            .await
            .unwrap();
    }
}

pub async fn unknow(connection: &mut Connection) {
    connection
        .write_frame(&Frame::Simple("Unknown Comamnd".to_string()))
        .await
        .unwrap();
}

// 给key增加过期时间TTL
pub async fn expire(
    connection: &mut Connection,
    element: &Vec<Frame>,
    db_clone: &Arc<Mutex<HashMap<String, Entry>>>,
    aof_file: &Arc<AOF>,
) {
    let mut db_lock = db_clone.lock().await;
    if let (Some(Frame::Bulk(key_element)), Some(Frame::Bulk(ttl_element))) =
        (element.get(1), element.get(2))
    {
        // 如果有这个key,则更新这个key的TTL,并返回Integer
        let key = String::from_utf8_lossy(key_element).to_string();
        let ttl_result = String::from_utf8_lossy(ttl_element).parse::<u64>();
        if db_lock.contains_key(&key)
            && let Some(entry) = db_lock.get(&key)
            && let Ok(ttl) = ttl_result
        {
            let new_ttl = Some(Instant::now() + Duration::from_secs(ttl));
            let new_entry = Entry {
                data: entry.data.clone(),
                ttl: new_ttl,
            };
            db_lock.insert(key, new_entry);
            connection.write_frame(&Frame::Integer(1)).await.unwrap();
            let cmd = format!(
                "EXPIRE {} {}\r\n",
                String::from_utf8_lossy(key_element),
                ttl
            );
            aof_file.log(&cmd).await;
        } else {
            // 没有这个key，直接返回Null
            connection.write_frame(&Frame::Null).await.unwrap();
        }
    }
}

pub async fn keys(connection: &mut Connection, db_clone: &Arc<Mutex<HashMap<String, Entry>>>) {
    let db_lock = db_clone.lock().await;
    let keys = db_lock
        .keys()
        .map(|k| Frame::Bulk(Bytes::from(k.as_bytes().to_vec())))
        .collect();
    connection.write_frame(&Frame::Array(keys)).await.unwrap();
}

pub async fn setnx(
    connection: &mut Connection,
    element: &Vec<Frame>,
    db_clone: &Arc<Mutex<HashMap<String, Entry>>>,
    aof_file: &Arc<AOF>,
) {
    if let (Some(Frame::Bulk(key)), Some(Frame::Bulk(value))) = (element.get(1), element.get(2)) {
    let k = String::from_utf8_lossy(key).to_string();
    let mut db_lock = db_clone.lock().await;
    if db_lock.get(&k).is_none() {
        let entry = Entry {
            data: value.clone(),
            ttl: None,
        };
        db_lock.insert(k.to_string(), entry);
        connection
            .write_frame(&Frame::Simple("OK".to_string()))
            .await
            .unwrap();
        let cmd = format!("SETNX {} {}\r\n", k, String::from_utf8_lossy(value));
        aof_file.log(&cmd).await;
    }else {
        connection
            .write_frame(&Frame::Null)
            .await
            .unwrap();

    }
    }
}


pub async fn setxx(
    connection: &mut Connection,
    element: &Vec<Frame>,
    db_clone: &Arc<Mutex<HashMap<String, Entry>>>,
    aof_file: &Arc<AOF>,
) {
    if let (Some(Frame::Bulk(key)), Some(Frame::Bulk(value))) = (element.get(1), element.get(2)) {
    let k = String::from_utf8_lossy(key).to_string();
    let mut db_lock = db_clone.lock().await;
    if db_lock.get(&k).is_some() {
        let entry = Entry {
            data: value.clone(),
            ttl: None,
        };
        db_lock.insert(k.to_string(), entry);
        connection
            .write_frame(&Frame::Simple("OK".to_string()))
            .await
            .unwrap();
        let cmd = format!("SETXX {} {}\r\n", k, String::from_utf8_lossy(value));
        aof_file.log(&cmd).await;
    }else {
        connection
            .write_frame(&Frame::Null)
            .await
            .unwrap();

    }
    }
}
