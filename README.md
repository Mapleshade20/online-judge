# 在线编程评测系统 (Online Judge)

[![License](https://img.shields.io/badge/license-AGPLv3-blue.svg)](LICENSE)
[![Rust Version](https://img.shields.io/badge/Rust-2024-orange.svg)](https://www.rust-lang.org/)

一个使用 Rust 开发的现代化在线编程评测系统，支持多语言编程题目的安全编译和执行。

## 🚀 项目特色

- **🛡️ 安全沙盒**: 基于 isolate 项目的安全隔离环境，支持 CPU、内存、I/O 资源限制和读写权限控制
- **🌐 异步架构**: Web 采用 Actix-Web 异步框架，支持高并发处理
- **📊 持久化存储**: 基于 SQLite 数据库，支持评测记录、用户数据的持久化
- **🔄 非阻塞评测**: 评测采用异步 Worker 池，支持多线程并发评测，评测任务队列管理
- **📈 实时排行榜**: 支持多种排序策略的动态排行榜系统
- **🔧 多语言支持**: 支持 Rust、C/C++ 等多种编程语言的评测
- **⚙️ 进程管理**: 沙盒命令运行结束或超时时自动清理进程
- **现代 Rust 生态**: 使用最新的 Rust 2024 Edition 和成熟的生态系统

## 🏗️ 系统架构

```
┌─────────────────┐    ┌──────────────────┐J-ID┌─────────────────┐
│   Web Frontend  │<-->│   HTTP Server    │--->│   Job Queue     │
│(not implemented)│    │   (Actix-Web)    │    │   (Tokio)       │
└─────────────────┘    └──────────────────┘    └─────────────────┘
                                | Full-Job              │ J-ID
                                |                       V
                       ┌──────────────────┐    ┌─────────────────┐
                       │    Database      │<-->│   Worker Pool   │
                       │    (SQLite)      │Full│   (Sandbox)     │
                       └──────────────────┘Job └─────────────────┘
                                                        │ Full-Job
                                                        V
                                               ┌─────────────────┐
                                               │  Isolate Runner │
                                               │  (Linux Sandbox)│
                                               └─────────────────┘
```

### 核心模块

- **`web_server.rs`**: HTTP 服务器配置和路由管理
- **`routes.rs, routes/`**: RESTful API 端点实现
- **`database.rs`**: SQLite 数据库操作和数据模型
- **`sandbox.rs, sandbox/`**: 安全沙盒环境管理
- **`queue.rs`**: 异步任务队列系统
- **`worker.rs`**: 评测工作线程池
- **`config.rs`**: 配置文件解析和管理

## 📦 技术栈

### 后端框架与库
- **[Actix-Web](https://actix.rs/)**: 高性能异步 Web 框架
- **[SQLx](https://github.com/launchbadge/sqlx)**: 异步 SQL 数据库驱动
- **[Tokio](https://tokio.rs/)**: 异步运行时
- **[Serde](https://serde.rs/)**: JSON 序列化/反序列化
- **[Clap](https://clap.rs/)**: 命令行参数解析

### 系统依赖
- **[Isolate](https://github.com/ioi/isolate)**: 安全沙盒运行环境
- **SQLite3**: 轻量级数据库
- **编译器**: rustc、gcc、g++ 等

## 🛠️ 快速开始

### 环境要求

- Linux 系统 (需要发行版有 systemd 且内核支持 cgroup v2，已测试 Ubuntu 24.04 和 Arch Linux ARM)
- Rust 1.75+ (2024 Edition)
- root 权限 (仅用于安装 Isolate)

### 1. 安装系统依赖

```bash
# 更新系统包
sudo apt update && sudo apt upgrade -y

# 安装基础开发工具
sudo apt install -y git make gcc g++ pkg-config
sudo apt install -y libcap-dev libsystemd-dev
sudo apt install -y libssl-dev sqlite3 libsqlite3-dev
```

### 2. 安装 Isolate 沙盒环境

> ⚠️ **安全警告**: 以下安装脚本需要切换至 root 用户，请仔细检查后再执行

```bash
# 检查 cgroup v2 支持 (必需)
[ -f /sys/fs/cgroup/cgroup.controllers ] && echo "cgroup v2 (unified) present" || echo "no cgroup v2"

# 切换到 root 用户执行安装
sudo su -

# 下载并编译 Isolate
cd /root
git clone https://github.com/ioi/isolate.git --depth=1
cd isolate
make isolate
make install

# 启动 Isolate 服务
systemctl daemon-reload
systemctl enable --now isolate.service

# 检查服务状态
systemctl status isolate.service

# 检查环境配置 (再根据输出进行必要调整)
isolate-check-environment

exit
```

### 3. 配置 Rust 编译环境

> ⚠️ **安全警告**: 以下安装脚本需要切换至 root 用户，请仔细检查后再执行

```bash
# 为 OJ 配置独立的 Rust Toolchain
sudo su -
mkdir -p /opt/oj/rust
export CARGO_HOME=/opt/oj/rust/cargo
export RUSTUP_HOME=/opt/oj/rust/rustup

# 在中国大陆建议使用镜像源加速
export RUSTUP_DIST_SERVER="https://rsproxy.cn"
export RUSTUP_UPDATE_ROOT="https://rsproxy.cn/rustup"

# 安装 Rust Toolchain
curl --proto '=https' --tlsv1.2 -sSf https://rsproxy.cn/rustup-init.sh | sh

# 配置 Cargo 镜像源
cat > $CARGO_HOME/config.toml << 'EOF'
[source.crates-io]
replace-with = 'rsproxy-sparse'
[source.rsproxy]
registry = "https://rsproxy.cn/crates.io-index"
[source.rsproxy-sparse]
registry = "sparse+https://rsproxy.cn/index/"
[registries.rsproxy]
index = "https://rsproxy.cn/crates.io-index"
[net]
git-fetch-with-cli = true
[build]
jobs = 4
EOF

# 设置权限
chmod -R 755 /opt/oj

exit
```

### 4. 验证沙盒环境 (建议以非 root 用户运行)

```bash
# 初始化测试沙盒
isolate -b 3 --cg --init

# 创建测试程序
cat > /tmp/test.rs << 'EOF'
fn main() {
    println!("Hello, Online Judge!");
}
EOF

# 复制到沙盒
cp /tmp/test.rs /var/local/lib/isolate/3/box/

# 测试编译 (Ubuntu 需要 --dir=/etc/alternatives，其他发行版如果报错"不存在路径"，就去掉该参数)
isolate -b 3 --cg --run --processes=10 --open-files=512 --fsize=65536 \
    --wall-time=30 --cg-mem=262144 \
    --dir=/opt/oj --dir=/etc/alternatives \
    -E RUSTUP_HOME=/opt/oj/rust/rustup -E CARGO_HOME=/opt/oj/rust/cargo \
    -E PATH=/opt/oj/rust/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
    --stderr-to-stdout -o compile.out -M /tmp/box3.meta -- \
    /bin/sh -c 'rustc -o main test.rs'

# 测试运行
isolate -b 3 --cg --run --processes=4 --open-files=30 --fsize=16384 \
    --time=1 --wall-time=5 --extra-time=1 --cg-mem=131072 --stack=65536 \
    -E PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
    --stderr-to-stdout -o case.out -M /tmp/box3.meta -- ./main

# 检查执行结果
cat /tmp/box3.meta
cat /var/local/lib/isolate/3/box/case.out

# 清理测试环境
isolate -b 3 --cg --cleanup
```

### 5. 编译和运行 OJ 系统

```bash
git clone <repository-url>
cd online-judge && cargo build --release
./target/release/oj --config data/example.json

# 或者使用 Cargo 运行
cargo run --release -- --config data/example.json
```

### 6. 测试 API 接口

```bash
# 创建用户
curl -X POST http://localhost:12345/users \
  -H "Content-Type: application/json" \
  -d '{"id": 1, "name": "testuser"}'

# 提交代码
curl -X POST http://localhost:12345/jobs \
  -H "Content-Type: application/json" \
  -d '{
    "source_code": "fn main() { println!(\"Hello World!\"); }",
    "language": "rust",
    "user_id": 1,
    "contest_id": 0,
    "problem_id": 0
  }'

# 查看评测结果
curl http://localhost:12345/jobs/0
```

## 📝 配置文件

系统使用 JSON 格式的配置文件，格式见 `data/example.json`。目前支持 Rust, C++, C 的编译，通过安装和配置其他工具链可拓展至大部分编译型语言。

## 🧪 运行测试

```bash
cargo test --test basic_requirements -- --test-threads=1

cargo test --test advanced_requirements -- --test-threads=1
```

## 📖 API 文档

系统提供 RESTful API，已实现的包括：

### 评测管理
- `POST /jobs` - 提交评测任务
- `GET /jobs` - 获取评测列表
- `GET /jobs/{id}` - 获取评测详情
- `PUT /jobs/{id}` - 重新评测

### 用户管理
- `GET /users` - 获取用户列表
- `POST /users` - 创建/更新用户

### 排行榜
- `GET /contests/{id}/ranklist` - 获取排行榜

详细的 API 文档请参考 `misc/api.md` 中对应的已实现部分。

## 🔧 命令行参数

```bash
oj [OPTIONS] --config <CONFIG>

OPTIONS:
    -c, --config <CONFIG>   配置文件路径
    -f, --flush-data        启动时清除数据库
    -t, --threads <NUM>     并发评测数量 (default: 2)
    -v, --verbose           详细日志输出
    -h, --help              显示帮助信息
```

## 🐛 故障排除

1. 端口被占用 (已有 OJ 在后台运行)

2. `isolate` 权限问题 (同 id 沙箱被其他用户创建后未清理)

3. 编译依赖问题 (见上方"快速开始")

4. 数据库问题 (尝试 `--flush-data`)

## 📄 许可证和致谢

本项目采用 AGPL v3 许可证——详情请见 [LICENSE](LICENSE) 文件。

项目仅用于学习和研究目的。在生产环境中使用前，请进行充分的安全评估和测试。

致谢以下项目的所有开发者:

- [Isolate](https://github.com/ioi/isolate) - 提供安全沙盒环境
- [Actix-Web](https://actix.rs/) - 现代异步 Web 框架
- [Rust 社区](https://www.rust-lang.org/community) - 优秀的生态系统

Todo:

- [x] 使用 `cargo fmt` 格式化代码
- [x] 编写适当的单元测试和集成测试
- [ ] 编写一系列恶意代码 (运行期、编译器、联网) 进行测试
- [ ] 编写适当的注释
- [ ] 把所有 u32, u64, i64, f64 等 arbitrary types 统一到一个位置定义
- [ ] 检查火焰图和内存占用，进行性能调优