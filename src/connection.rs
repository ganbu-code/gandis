use crate::frame::Frame;
use bytes::{Buf, BytesMut};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

// 封装网络连接及对应的读取写入方法
pub struct Connection {
    // 负责网络读写
    stream: TcpStream,
    // 专属于这个网络连接的缓冲区
    buf: BytesMut,
}
// Connection的方法
impl Connection {
    pub fn new(stream: TcpStream) -> Connection {
        Connection {
            stream,
            buf: BytesMut::with_capacity(1024),
        }
    }

    pub async fn read_frame(&mut self) -> Result<Option<Frame>, String> {
        loop {
            if let Some(frame) = self.parse_frame()? {
                return Ok(Some(frame));
            }

            if let Some(pos) = self.buf.iter().position(|&b| b == b'\n') {
                let line = self.buf.split_to(pos + 1);
                let text = String::from_utf8_lossy(&line).trim().to_string();
                if text.is_empty() {
                    continue;
                }
                let parts: Vec<Frame> = text
                    .split_whitespace()
                    .map(|s| Frame::Bulk(bytes::Bytes::from(s.to_string())))
                    .collect();

                return Ok(Some(Frame::Array(parts)));
            }

            let n = match self.stream.read_buf(&mut self.buf).await {
                Ok(n) => n,
                Err(_) => {
                    return Err("error connection closed".to_string());
                }
            };

            if n == 0 {
                if self.buf.is_empty() {
                    return Ok(None);
                } else {
                    return Err("error connection closed".to_string());
                }
            }
        }
    }

    pub fn parse_frame(&mut self) -> Result<Option<Frame>, String> {
        let target = self.find_target();
        let target_index = match target {
            Some(target) => target,
            None => return Ok(None),
        };
        match self.buf[0] {
            b'+' => {
                let line = self.buf.split_to(target_index + 2);
                let content = &line[1..line.len() - 2];
                let content_result = String::from_utf8_lossy(content).to_string();
                Ok(Some(Frame::Simple(content_result)))
            }
            b'*' => {
                let target = match self.find_target() {
                    Some(target_index) => target_index,
                    None => return Ok(None),
                };
                let line = self.buf.split_to(target + 2);
                let content = String::from_utf8_lossy(&line[1..line.len() - 2])
                    .parse::<usize>()
                    .map_err(|_| "cannot parse")?;
                let mut elements = Vec::new();
                for _ in 0..content {
                    match self.parse_frame()? {
                        Some(frame) => elements.push(frame),
                        None => return Ok(None),
                    };
                }
                Ok(Some(Frame::Array(elements)))
            }
            b'$' => {
                // 找到\r\n
                let target = match self.find_target() {
                    Some(n) => n,
                    None => return Ok(None),
                };
                let line = self.buf.split_to(target + 2);
                let content_result = String::from_utf8_lossy(&line[1..line.len() - 2])
                    .parse::<usize>()
                    .map_err(|_| "connot find length")?;
                if self.buf.len() < content_result {
                    return Ok(None);
                }
                let data = self.buf.split_to(content_result);
                self.buf.advance(2);
                Ok(Some(Frame::Bulk(data.freeze())))
            }
            _ => Ok(None),
        }
    }

    pub async fn write_frame(&mut self, frame: &Frame) -> Result<(), std::io::Error> {
        match frame {
            Frame::Simple(text) => {
                self.stream.write_all(text.as_bytes()).await?;
                self.stream.write_all("\r\n".as_bytes()).await?;
            }
            Frame::Error(text) => {
                self.stream.write_all("-".as_bytes()).await?;
                self.stream.write_all(text.as_bytes()).await?;
                self.stream.write_all("\r\n".as_bytes()).await?;
            }
            Frame::Bulk(data) => {
                self.stream.write_all(data).await?;
                self.stream.write_all("\r\n".as_bytes()).await?;
            }
            Frame::Array(data) => {
                println!("Array: {:?}", data);
                if data.len() == 0 {
                    self.stream.write_all("Null\r\n".as_bytes()).await?;
                } else {
                    for element in data {
                        Box::pin(self.write_frame(element)).await?;
                    }
                }
            }
            Frame::Null => {
                self.stream.write_all("Null\r\n".as_bytes()).await?;
            }
            Frame::Integer(data) => {
                self.stream.write_all("(Integer)".as_bytes()).await?;
                self.stream.write_all(data.to_string().as_bytes()).await?;
                self.stream.write_all("\r\n".as_bytes()).await?;
            }
        }
        self.stream.flush().await
    }

    pub fn find_target(&mut self) -> Option<usize> {
        for i in 0..self.buf.len() {
            if self.buf[i] == b'\r' && i + 1 < self.buf.len() && self.buf[i + 1] == b'\n' {
                return Some(i);
            }
        }
        None
    }
}
