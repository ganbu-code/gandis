use clap::Parser;
use bytes::Bytes;
use gandis::connection::Connection;
use gandis::frame::Frame;
use tokio::net::TcpStream;
use std::io::{self, Write};
use gandis::DEFAULT_PORT;

#[derive(Debug, Clone, Parser)]
#[command(name = "client", about = "A simple client for the server")]
struct Args {
    #[arg(short, long, default_value = "127.0.0.1")]
    ip: String,
    #[arg(short, long, default_value = DEFAULT_PORT)] 
    port: u64,
}

#[tokio::main]
async fn main() -> tokio::io::Result<()> {
    let arg = Args::parse();
    let addr = format!("{}:{}", arg.ip, arg.port);
    let stream = TcpStream::connect(&addr).await?;
    let mut connection = Connection::new(stream);
    println!("成功连接至服务器: {}", addr);
    println!("输入命令 (例如: SET key value), 输入 exit 退出");
    loop {
        print!("gandis-cli> ");
        io::stdout().flush()?; 
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let cmd = input.trim();
        if cmd == "exit" {
            break;
        }
        if cmd.is_empty() {
            continue;
        }
        let req = human_command_to_resp(cmd);
        if connection.write_frame(&req).await.is_err() {
            println!("请求发送失败");
            break;
        }
        match connection.read_frame().await {
            Ok(Some(frame)) => println!("{}", render_resp_for_human(&frame)),
            Ok(None) => {
                println!("服务端关闭了连接");
                break;
            }
            Err(err) => {
                println!("读取响应失败: {}", err);
                break;
            }
        }
    }
    Ok(())
}

fn human_command_to_resp(input: &str) -> Frame {
    let parts: Vec<Frame> = input
        .split_whitespace()
        .map(|s| Frame::Bulk(Bytes::from(s.to_string())))
        .collect();
    Frame::Array(parts)
}

fn render_resp_for_human(frame: &Frame) -> String {
    match frame {
        Frame::Simple(text) => text.to_string(),
        Frame::Error(text) => format!("(error) {}", text),
        Frame::Integer(v) => format!("(integer) {}", v),
        Frame::Bulk(v) => String::from_utf8_lossy(v).to_string(),
        Frame::Null => "(nil)".to_string(),
        Frame::Array(items) => {
            if items.is_empty() {
                return "(empty array)".to_string();
            }
            let mut lines = Vec::with_capacity(items.len());
            for (idx, item) in items.iter().enumerate() {
                lines.push(format!("{}) {}", idx + 1, render_resp_for_human(item)));
            }
            lines.join("\n")
        }
    }
}
