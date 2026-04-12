use clap::Parser;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
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
    let mut stream = TcpStream::connect(&addr).await?;
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
        let send_data = format!("{}\r\n", cmd);
        stream.write_all(send_data.as_bytes()).await?;
        let mut buf = [0u8; 1024];
        let n = stream.read(&mut buf).await?;
        if n == 0 {
            println!("服务端关闭了连接");
            break;
        }
        print!("{}", String::from_utf8_lossy(&buf[..n]));
    }
    Ok(())
}