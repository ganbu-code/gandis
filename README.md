# gandis: 一个更简单的 mini-redis 学习项目

这是一个用 Rust + Tokio 实现的类 Redis 学习项目。
目标不是一次做完全部 Redis，而是通过逐步演进掌握：
- 网络协议（RESP）
- 异步 I/O
- 并发与共享状态
- 持久化（AOF）
- 多数据结构命令实现

## 1. 学习历程（建议按这个顺序读代码）

1. **先从连接层入手**
- 看 `frame.rs` 和 `connection.rs`，理解 RESP 是怎么被解析/编码的。
- 关键目标：服务端只处理 RESP 
 
2. **再看服务端主循环**
- 看 `src/bin/server.rs`：`accept -> spawn -> read_frame`。
- 理解每个连接一个异步任务，以及命令如何分发到 `handle.rs`。

3. **再看状态存储模型**
- 看 `db.rs`：`data` 支持 `String`。
- `Entry` 包含 `ttl`，用于过期逻辑。

4. **再看命令实现**
- 看 `handle.rs`：
  - String: `SET/GET/DEL/EXPIRE/...`

5. **最后看持久化**
- 看 `aof.rs`：写入命令日志 + 启动重放。
- 理解 AOF 重放与命令执行层的关系（复用存储逻辑）。

## 2. 涉及的核心知识点

- Rust 枚举建模：`Frame`、`Data`
- 异步网络编程：TCP server/client
- 并发共享状态：`Arc<Mutex<...>>`
- 协议设计与实现：RESP 请求/响应
- 过期策略：惰性删除 + 定时清理
- 持久化：AOF append + replay

---

## 3. Tokio 重点讲解（本项目涉及的全部关键点）

### 3.1 `#[tokio::main]`：异步运行时入口
`server.rs` 和 `client.rs` 都使用 `#[tokio::main]`，它会：
- 创建 Tokio Runtime
- 启动调度器
- 允许在 `main` 中直接 `await`

这让你不需要手动创建 runtime。

### 3.2 Tokio 网络 API：`TcpListener` / `TcpStream`
- `TcpListener::bind(...).await`：异步绑定端口
- `listener.accept().await`：异步接受新连接
- `TcpStream::connect(...).await`：异步建立客户端连接

区别于阻塞 I/O：等待网络期间不会阻塞线程，可调度其他任务继续执行。

### 3.3 任务模型：`tokio::spawn`
在 `server.rs` 中，每接收一个连接都会 `spawn` 一个独立任务处理：
- 一个连接慢，不影响其他连接
- 提高并发处理能力

这是 Tokio 最常见的“每连接一任务”模型。

### 3.4 异步读写：`AsyncReadExt` / `AsyncWriteExt`
在 `connection.rs` 中：
- `read_buf` 读取 socket 数据到 `BytesMut`
- `write_all` 写回 RESP 编码后的响应
- `flush` 强制刷出缓冲

这构成了协议层的数据通道。

### 3.5 时间相关：`tokio::time::sleep` 与 `Instant`
项目用到了两类时间：
- `sleep`：后台定时任务每秒扫描过期 key
- `Instant`：记录绝对到期时间并判断是否过期

这是典型 TTL 实现方式。

### 3.6 异步同步原语：`tokio::sync::Mutex`
数据库 `Db` 是：`Arc<Mutex<HashMap<...>>>`
- `Arc`：跨任务共享所有权
- `Mutex`：异步锁，`lock().await` 获取

要点：
- 尽量缩短持锁时间
- 避免在持锁状态下做无关 await

### 3.7 Tokio 文件 I/O：`tokio::fs` + 异步写文件
AOF 使用：
- `OpenOptions::open(...).await`
- `write_all(...).await`
- `flush().await`
- `tokio::fs::read(...).await`（重放）

这使持久化不阻塞网络处理线程。

### 3.8 为什么选择 `tokio` 而不是线程 + 阻塞 I/O
因为 mini-redis 场景本质是 I/O 密集：
- 网络读写频繁
- 多连接并发
- 有周期任务（GC）和磁盘 I/O（AOF）

Tokio 让这些任务共享线程池并高效切换。

## 4. 快速运行

```bash
cargo run --bin server
```

另开终端：

```bash
cargo run --bin client
```

示例：

```text
SET k v
GET k
KEYS
DEL k
```