use clap::Parser;
use gandis::AOF_FILE;
use gandis::DEFAULT_PORT;
use gandis::aof::AOF;
use gandis::connection::Connection;
use gandis::db::Db;
use gandis::frame::Frame;
use gandis::handle::*;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::{net::TcpListener, time::Instant};

#[derive(Debug, Clone, Parser)]
#[command(name = "client", about = "A simple client for the server")]
struct Args {
    #[arg(short, long, default_value = "127.0.0.1")]
    ip: String,
    #[arg(short, long, default_value = DEFAULT_PORT)]
    port: u64,
}

#[tokio::main]
pub async fn main() {
    let arg = Args::parse();
    let listener = TcpListener::bind(format!("{}:{}", arg.ip, arg.port))
        .await
        .unwrap();
    // 初始化全局内存
    let db: Db = Arc::new(Mutex::new(HashMap::new()));
    let aof = Arc::new(AOF::new(AOF_FILE).await);
    let aof_gc = aof.clone();
    let db_gc = db.clone();
    let db_aof = db.clone();
    // 这里实现一个AOF文件的读取并持久化进内存
    aof.clone().replay(&db_aof).await;
    //  开启一个GC任务，主动删除HashMap的过期键值
    tokio::spawn(async move {
        loop {
            let mut expire_keys: Vec<String> = Vec::new();
            tokio::time::sleep(Duration::from_secs(1)).await;
            db_gc.lock().await.retain(|key, value| {
                if let Some(ttl) = value.ttl {
                    let is_gc: bool = Instant::now() <= ttl;
                    if !is_gc {
                        expire_keys.push(key.clone());
                    }
                    is_gc
                } else {
                    true
                }
            });
            for key in expire_keys {
                aof_gc.log(&format!("DEL {}\r\n", key)).await;
            }
            expire_keys = Vec::new();
        }
    });

    loop {
        let (socket, _) = listener.accept().await.unwrap();
        let db_clone = db.clone();
        let aof_file = aof.clone();
        tokio::spawn(async move {
            let mut connection: Connection = Connection::new(socket);
            loop {
                // let (mut stream_reader, _) = socket.split();
                // fix: 使用分号 放在每个任务的内部使其有独立的buf
                // let mut buf = [0; 1024];
                match connection.read_frame().await {
                    Ok(Some(frame)) => {
                        println!("接收数据成功: {:?}", frame);
                        if let Frame::Array(element) = frame {
                            if let Some(Frame::Bulk(command)) = element.get(0) {
                                let cmd = String::from_utf8_lossy(command).to_uppercase();
                                match cmd.as_str() {
                                    "PING" => { ping(&mut connection).await; }
                                    "SET" => { set(&mut connection, &element, &db_clone, &aof_file).await; }
                                    "SETNX" => { setnx(&mut connection, &element, &db_clone, &aof_file).await; }
                                    "SETXX" => { setxx(&mut connection, &element, &db_clone, &aof_file).await; }
                                    "GET" => { get(&mut connection, &element, &db_clone).await; }
                                    "DEL" => { del(&mut connection, &element, &db_clone, &aof_file).await; }
                                    "KEYS" => { keys(&mut connection, &db_clone).await; }
                                    "EXPIRE" => { expire(&mut connection, &element, &db_clone, &aof_file).await; }
                                    _ => { unknow(&mut connection).await; }
                                }
                            }
                        }
                    }
                    Ok(None) => {
                        println!("main/loop/Ok(None)");
                        break;
                    }
                    Err(err) => {
                        println!("main/loop/Err: {:?}", err);
                        break;
                    }
                }
            }
        });
    }
}
