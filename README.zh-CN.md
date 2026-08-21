<p align="center">
  <img src="assets/logo.svg" width="200" height="200" alt="Nudge Logo">
</p>

<h1 align="center">Nudge</h1>

<p align="center">
  <strong>不要解析你的代理，引导它们。</strong><br>
  一个类型化、可重放、预算感知的 LLM 代理编程语言。<br>
  编译为 Python 和 TypeScript。
</p>

<p align="center">
  <img alt="版本" src="https://img.shields.io/badge/版本-1.2.0-blue">
  <img alt="许可证" src="https://img.shields.io/badge/许可证-专有-red">
  <img alt="平台" src="https://img.shields.io/badge/平台-Linux%20%7C%20macOS%20%7C%20Windows-green">
  <img alt="目标" src="https://img.shields.io/badge/目标-Python%20%7C%20TypeScript-yellow">
  <a href="https://marketplace.visualstudio.com/items?itemName=Nekomya.nudge-lang"><img alt="VS Code" src="https://img.shields.io/badge/VS%20Code-Nudge%20语言-007ACC?logo=visualstudiocode"></a>
</p>

---

<p align="center">
  <a href="README.md">English</a> •
  <a href="README.zh-CN.md">中文</a>
</p>

---

## 为什么选择 Nudge？

生产环境的代理仍然由胶水代码拼凑而成：手动解析的提示链、try/except 包装的工具调用、没有重放、没有成本控制、没有回归测试。库只是修补症状。**Nudge 从问题真正所在的层面解决：语言层面。**

| 痛点 | 库 | Nudge |
|---|---|---|
| 无类型的 LLM 输出 | 运行时验证 | 模式是类型 — 编译时证明 |
| 隐藏的副作用 | 不可见 | 每个签名中的 `uses LLM, Tool, IO` |
| 没有回归测试 | 事后添加记录/重放 | 每次运行产生跟踪；每个跟踪都是测试 |
| 成本意外 | 事后仪表板 | 预算是编译器+运行时强制执行的契约 |
| 异步扇出混乱 | 手动 asyncio | `par map / race / all`，编译时竞态安全 |

## Nudge 代码示例

```
type Finding = { claim: string, source: Url, confidence: float @range(0, 1) }

fn analyze(q: string, hits: [SearchResult]) -> [Finding] uses LLM {
    llm"""从 {hits} 中提取关于 {q} 的可验证发现"""
    with { schema: [Finding], model: "anthropic:sonnet-4.6",
           budget: 0.03 USD, retry: 2 with repair }
}

test "stays within budget on recorded trace" {
    let t = replay("traces/demo.jsonl")
    assert t.cost_usd < 0.25          // CI 中零 token 消耗
}
```

编译器证明模式匹配、推断效果并计算静态成本边界。运行时将每次调用记录到可寻址的跟踪中，你可以 diff、提交和重放。

## 功能特性

<div align="center">

| 功能 | 描述 |
|:---:|:---|
| **类型化 LLM 调用** | 输出模式是语言类型；违规触发自动修复 |
| **效果系统** | 纯函数 / `LLM` / `Tool` / `IO` 效果推断并显示在签名中 |
| **确定性重放** | 完整、混合和实时模式；跟踪是 git 友好的 JSONL |
| **预算契约** | 每次调用、每次运行和每次修复的 USD 上限，带静态估算 |
| **检查点恢复** | 崩溃后，从最后一个检查点 `nudge resume` |
| **原生并行** | `par map`、`par race`、`par all`，编译时竞态安全 |
| **提示 Clippy** | 编译器 lint 你的 `llm"""` 块：模糊指令、缺失契约 |
| **MCP 和 Python 互操作** | 通过 stdio 消费真实 MCP 服务器；可使用任何 pip 包 |
| **真实提供商** | OpenAI / Gemini / Groq / MiMo / Mistral / Anthropic / Ollama |
| **跟踪查看器** | 本地 Web UI：时间线、token、成本、修复高亮 |
| **跟踪差异** | 比较两个跟踪："编辑提示后什么改变了？" |
| **A2A 和 LSP 和 OTel** | 内置，非外挂 |

</div>

## 快速开始

### 安装

**一键安装（推荐）：**

```sh
# Linux / macOS
curl -fsSL https://raw.githubusercontent.com/NekomyaDev/nudge/main/install.sh | bash

# Windows（以管理员身份运行 PowerShell）
irm https://raw.githubusercontent.com/NekomyaDev/nudge/main/install.ps1 | iex
```

**包管理器：**

```sh
# Snap (Linux)
sudo snap install nudge --classic

# Docker
docker run -it --rm -v $(pwd):/workspace nekomyadev/nudge nudgec --help
```

**GUI 安装器（双击）：**

- **Windows：** 下载 [`install.bat`](https://github.com/NekomyaDev/nudge/releases/download/v1.2.0/install.bat) 并双击
- **macOS：** 下载 [`install.command`](https://github.com/NekomyaDev/nudge/releases/download/v1.2.0/install.command) 并双击

**手动安装：**

从 [Releases](https://github.com/NekomyaDev/nudge/releases) 页面下载：

### 你的第一个 Nudge 程序

```sh
# 创建程序
cat > hello.ndg << 'EOF'
type Greeting = { message: string, timestamp: string }

fn greet(name: string) -> Greeting uses LLM {
    llm"""为 {name} 创建一个问候。返回 message 和 timestamp。"""
    with { schema: Greeting, model: "anthropic:sonnet-4.6", budget: 0.01 USD }
}

test "greet works on recorded trace" {
    let t = replay("traces/greet.jsonl")
    assert t.output.message != ""
}
EOF

# 类型检查
nudgec check hello.ndg

# 编译为 Python
nudgec build hello.ndg

# 运行（无需 API 密钥 - 使用假提供商）
export PYTHONPATH=$PWD/runtime
python3 out/hello.py

# 运行测试（零 token）
nudgec test hello.ndg
```

默认情况下，所有内容都针对确定性假提供商运行：**无需 API 密钥，无需 token 消耗。**

## 后端对等性

| 功能 | Python | TypeScript |
|:---|:---:|:---:|
| 类型化调用、模式验证、修复 | ✅ | ✅ |
| 跟踪、重放、预算墙 | ✅ | ✅ |
| `par map/all/race` + 分支标签 | ✅ | ✅ |
| 流式传输（`stream let`） | ✅ | ✅ |
| 真实提供商 | ✅ | ⬜ |
| MCP 工具、检查点/恢复、OTel | ✅ | ⬜ |

## VS Code 扩展

安装 [Nudge Language](https://marketplace.visualstudio.com/items?itemName=Nekomya.nudge-lang) 扩展以获得：

- 语法高亮
- 代码片段
- 通过 `nudgec lsp` 的实时诊断
- 悬停信息
- 跳转到定义

## 隐私说明

跟踪逐字记录提示、模型输出和工具结果 — 它们可能包含机密或个人数据。将跟踪文件视为敏感工件；编辑挂钩已在路线图上。

## 许可证

专有 — 参见 [LICENSE](LICENSE) 和 [LICENSE-BINARY](LICENSE-BINARY)。

Nudge 免费使用但闭源。有关许可咨询，请联系 [@NekomyaDev](https://github.com/NekomyaDev)。

---

<p align="center">
  由 <a href="https://github.com/NekomyaDev">NekomyaDev</a> 用 ❤️ 制作
</p>
