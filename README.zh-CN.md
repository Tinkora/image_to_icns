# image_to_icns

[English](README.md) | [简体中文](README.zh-CN.md)

[![在 Ko-fi 上支持 Tinkora](https://ko-fi.com/img/githubbutton_sm.svg)](https://ko-fi.com/tinkora)

在浏览器中将 PNG、JPEG 或 SVG 图片转换为经过校验的 macOS `.icns`
文件。图片解码、裁剪、缩放、编码和校验均由 Rust 与 WebAssembly 在本地完成。

[![CI](https://github.com/Tinkora/image_to_icns/actions/workflows/test.yml/badge.svg)](https://github.com/Tinkora/image_to_icns/actions/workflows/test.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-15803d.svg)](LICENSE)
[![Rust 1.95+](https://img.shields.io/badge/rust-1.95%2B-000000.svg)](rust-toolchain.toml)

## 使用在线编辑器

打开[在线编辑器](https://tinkora.github.io/image_to_icns/)，选择图片、调整方形裁剪区域、
生成图标并下载 `icon.icns`。源图片不会离开当前浏览器标签页。

## 为什么需要这个工具

macOS 图标并不是改个扩展名的 PNG。一个 `.icns` 文件包含多种逻辑尺寸和 Retina
表示。`image_to_icns` 会从同一个 1024 px 主图生成并校验 10 个现代表示：

- 覆盖 16、32、64、128、256、512 和 1024 物理像素
- 每个尺寸都从主图独立使用 Lanczos3 缩放
- 下载前重新读取并校验 ICNS 内容
- 支持透明背景和可视化方形裁剪

## 当前范围

公开浏览器编辑器是主要产品，无需账户或后端即可使用，支持 PNG、JPEG 和 SVG 输入。

仓库还包含可选的、自托管的 MCP Session 服务。它可以创建短期编辑器链接、查询用户
是否正在编辑或已经完成转换，以及取消 Session。服务只在 Cloudflare D1 中保存元数据，
不会保存源图片或生成文件。MCP 流程不会把用户下载的 `.icns` 自动传回 Agent。

浏览器版本暂不支持 PDF、服务端图片处理或生成文件分享。

## 本地构建

需要：

- Rust 1.95
- `wasm32-unknown-unknown` target
- `wasm-pack` 0.15
- Python 3 或其他静态文件服务器

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-pack --version 0.15.0 --locked
./scripts/build_web.sh
python3 -m http.server 4173 --directory dist
```

然后打开 `http://127.0.0.1:4173`。

## 项目结构

| 路径 | 职责 |
| --- | --- |
| `crates/image_to_icns_core` | 解码、裁剪、渲染、ICNS 编码与校验 |
| `crates/image_to_icns_web` | WASM 绑定与浏览器编辑器 |
| `crates/image_to_icns_mcp` | 基于 stdio 的 MCP JSON-RPC 服务 |
| `crates/image_to_icns_worker` | 可选的 Cloudflare Worker 与 D1 Session 存储 |
| `scripts/build_web.sh` | 可重复执行的 Web 生产构建 |

静态编辑器运行时不依赖 MCP 服务或 Worker。

## 质量检查

```bash
bash scripts/test_validate_release.sh

cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
wasm-pack test --node crates/image_to_icns_core --locked

node --test crates/image_to_icns_web/tests/*.test.mjs
./scripts/build_web.sh

cargo fmt --manifest-path crates/image_to_icns_worker/Cargo.toml -- --check
cargo test --manifest-path crates/image_to_icns_worker/Cargo.toml --locked
wasm-pack test --node crates/image_to_icns_worker --locked
cargo clippy \
  --manifest-path crates/image_to_icns_worker/Cargo.toml \
  --target wasm32-unknown-unknown \
  --locked -- -D warnings

npx --yes markdownlint-cli2@0.20.0 '**/*.md'
go run github.com/rhysd/actionlint/cmd/actionlint@v1.7.7 \
  .github/workflows/*.yml
uvx zizmor==1.29.0 --collect workflows --offline .github/workflows
```

`cargo test --workspace --locked` 覆盖原生 workspace，但 `wasm_api.rs` 只为
`wasm32` 编译，因此在 host target 上实际执行 0 项测试。独立的 Core
`wasm-pack` 命令会在 Node 中执行 9 项测试，覆盖 PNG、JPEG 与 SVG 解码、未知格式和
损坏输入拒绝、默认渲染、非方形编码拒绝、编码与校验往返，以及截断数据校验失败。
Worker 的 `wasm-pack` 命令则单独检查其 WebAssembly `Response` 边界。

## 可选 MCP Session

MCP 二进制需要已经部署的 Session Worker，默认连接本机
`http://localhost:8787`。Editor 部署还必须在可信的 `config.js` 中固定同一个
Worker origin；Session 链接不能自行选择任意请求目标。

```json
{
  "mcpServers": {
    "image_to_icns": {
      "command": "image_to_icns_mcp",
      "args": ["--worker-url", "https://your-worker.example.com"]
    }
  }
}
```

D1 schema、本地开发和部署步骤见[自托管 Session Worker](docs/SELF_HOSTING.md)。

## 隐私与安全

- 源图片和生成的 `.icns` 字节只存在于浏览器内存中。
- Session 记录仅包含不透明 ID、secret 哈希、时间戳、状态、源格式提示、输出大小和表示数量；
  转换失败时还可能记录不含敏感信息的错误代码。
- Session secret 使用 64 个随机字节，并放在 URL fragment 中；浏览器不会在 HTTP 请求或
  referrer header 中发送 fragment。
- Session 元数据 30 分钟后过期；定时清理任务会在终态记录保留 24 小时后将其删除。

请通过 [GitHub 私有 Security Advisory](https://github.com/Tinkora/image_to_icns/security/advisories/new)
报告漏洞，范围和披露方式见 [SECURITY.md](SECURITY.md)。

## 文档

- [贡献状态与开发指南](CONTRIBUTING.md)
- [自托管指南](docs/SELF_HOSTING.md)
- [ADR-0001：浏览器优先架构](docs/decisions/0001-web-first-session-architecture.md)
- [ADR-0002：首个版本仅本地处理](docs/decisions/0002-local-only-first-release.md)
- [变更日志](CHANGELOG.md)

## 许可证

MIT License，详见 [LICENSE](LICENSE)。
