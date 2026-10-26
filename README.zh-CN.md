<p align="center">
  <img src="assets/logo.svg" width="200" height="200" alt="Nudge Logo">
</p>

<h1 align="center">Nudge</h1>

<p align="center">
  <strong>不要解析你的代理，引导它们。</strong><br>
  面向 AI 智能体的类型安全、可重放、具备预算感知的编程语言。<br>
  原生字节码运行时与独立编译器。可编译为 Python 和 TypeScript。
</p>

<p align="center">
  <img alt="Version" src="https://img.shields.io/badge/version-1.5-blue">
  <img alt="License" src="https://img.shields.io/badge/license-Apache--2.0-green">
  <img alt="Platform" src="https://img.shields.io/badge/platform-Linux%20%7C%20macOS%20%7C%20Windows-green">
  <img alt="Target" src="https://img.shields.io/badge/target-Native%20%7C%20Python%20%7C%20TypeScript%20%7C%20WASM-yellow">
  <a href="https://marketplace.visualstudio.com/items?itemName=Nekomya.nudge-lang"><img alt="VS Code" src="https://img.shields.io/badge/VS%20Code-Nudge%20Language-007ACC?logo=visualstudiocode"></a>
  <a href="https://snyk.io"><img alt="Snyk Verified" src="https://img.shields.io/badge/Snyk-Verified%20Secure-4C556A?logo=snyk"></a>
</p>

---

<p align="center">
  <a href="README.md">English</a> •
  <a href="README.zh-CN.md">中文</a> •
  <a href="README.ja-JP.md">日本語</a> •
  <a href="README.ko-KR.md">한국어</a> •
  <a href="README.ru-RU.md">Русский</a> •
  <a href="README.es-ES.md">Español</a> •
  <a href="README.tr-TR.md">Türkçe</a> •
  <a href="README.de-DE.md">Deutsch</a> •
  <a href="README.fr-FR.md">Français</a>
</p>

---

## 为什么选择 Nudge？

生产环境中的智能体大多仍依赖胶水代码维系：手动解析提示词字符串、使用 try/except 包裹工具调用、缺乏重放能力、不可预知的成本，以及缺失回归测试。库只是在问题发生后修补表象。**Nudge 从问题真正发生的层面进行解决：语言本身。**

| 痛点 | 常规开发库 | Nudge |
|:---|:---|:---|
| 无类型的 LLM 输出 | 运行时验证 | 模式是语言一等类型，编译期证明 |
| 隐式副作用 | 不透明 | 签名中声明 10 种显式类型化 Effect (`uses LLM, Tool, IO, ...`) |
| 缺乏回归测试 | 依靠外挂录制与回放 | 每次运行都写入 NTF 跟踪，每个跟踪都是确定性的 $0 测试 |
| 意外成本失控 | 收到账单后才监控 | 预算是编译器和运行时强制执行的硬性契约 |
| 混乱的并发逻辑 | 手动异步循环 | 原生 `par map / race / all`，具备编译期竞态安全保证 |

## Nudge 代码体验

```nudge
type Finding = { claim: string, source: Url, confidence: float @range(0, 1) }

fn analyze(q: string, hits: [SearchResult]) -> [Finding] uses LLM {
    llm"""Extract verifiable findings about {q} from: {hits}"""
    with { schema: [Finding], model: "anthropic:sonnet-4.6",
           budget: 0.03 USD, retry: 2 with repair }
}

test "stays within budget on recorded trace" {
    let t = replay("traces/demo.jsonl")
    assert t.cost_usd < 0.25          // CI 中零 token 消耗
}
```

编译器证明模式匹配、推断副作用并约束最坏情况成本。运行时将每次调用记录到基于内容寻址的跟踪中，你可以进行 diff、提交到版本库，并以 $0 成本进行重放。

## 快速开始

### 1. 安装 Nudge

**一键安装脚本（推荐）：**

```sh
# Linux / macOS
curl -fsSL https://raw.githubusercontent.com/NekomyaDev/nudge/main/install.sh | bash

# Windows (以管理员身份运行 PowerShell)
irm https://raw.githubusercontent.com/NekomyaDev/nudge/main/install.ps1 | iex
```

**包管理器与容器：**

```sh
# Snap (Linux)
sudo snap install nudge --classic

# Docker
docker run -it --rm -v $(pwd):/workspace nekomyadev/nudge nudgec --help
```

Linux、macOS 和 Windows 的预编译二进制文件也可以在 [Releases](https://github.com/NekomyaDev/nudge/releases) 页面获取。

### 2. 60 秒极速初体验（原生，无需 API Key，$0 成本）

无需 Python，无需 Node.js，无需 API 密钥。Nudge 内置原生栈式虚拟机和内存确定性仿真提供商：

```sh
# 创建程序
cat > hello.ndg << 'EOF'
type Greeting = { message: string, timestamp: string }

fn greet(name: string) -> Greeting uses LLM {
    llm"""Create a greeting for {name}. Return message and timestamp."""
    with { schema: Greeting, model: "anthropic:sonnet-4.6", budget: 0.01 USD }
}

fn main() -> Greeting uses LLM {
    greet("World")
}
EOF

# 类型检查程序
nudgec check hello.ndg

# 在原生 Bytecode VM 上直接运行（$0 成本，无需密钥）
nudgec run hello.ndg

# 运行离线回归测试
nudgec test hello.ndg
```

### 3. 交互式课程与项目脚手架

```sh
# 终端内的交互式教学课程 (hello, types, decide, policy, test, trace)
nudgec learn

# 基于就绪模板生成项目脚手架 (triage, chatbot, rag-agent 等)
nudgec init my-agent --template triage
cd my-agent && nudgec check my-agent.ndg && nudgec run my-agent.ndg
```

### 4. 编译至 Python 或 TypeScript（可选）

如需集成部署到现有的 Python 或 TypeScript 服务中：

```sh
# Python 目标
pip install nudge-runtime          # 仅依赖标准库，零外部依赖
nudgec build hello.ndg              # 生成 out/hello.py
python3 out/hello.py

# TypeScript 目标
nudgec build-ts hello.ndg             # 生成 out/hello.ts
```

## 核心特性一览

| 特性 | 提供能力 | 章节 |
|:---|:---|:---|
| **类型化 LLM 调用** | 输出模式即语言类型，违规自动触发多轮修复 | [Nudge 代码体验](#nudge-代码体验) |
| **控制流与类型** | `if/else`、`for` 循环、记录字面量 `{k: v}`、索引、可变局部变量、Optional (`T?`)、Union (`T \| U`) | [控制流与数据](#控制流与数据) |
| **类型化 Effect** | 编译期追踪与验证的 10 种已知 Effect | [类型化 Effect](#类型化-effect) |
| **Workspace 与 Store** | 沙箱隔离的作用域文件系统 + 类型化持久键值存储 | [作用域 Workspace 与持久化 Store](#作用域-workspace-与持久化-store) |
| **本地 UI** | 类型化视图、窗口、Headless 测试与原生桌面窗口 | [本地 UI](#本地-ui) |
| **任务与并发** | 任务 spawn/join、协作式取消、安全 `par race`、预算即时释放 | [任务生命周期与并发](#任务生命周期与并发) |
| **提供商认证** | Fail-closed RFC 8628 OAuth 设备流，严禁外部 token 爬取 | [提供商认证](#提供商认证) |
| **类型化决策** | 状态批量分类、Yes/No 判断、评分规则 (`/v1/systemone`) | [类型化决策](#类型化决策) |
| **计算机操作** | 屏幕观察、类型化动作、应用白名单、硬性终止开关 | [计算机操作 (Computer Use)](#计算机操作-computer-use) |
| **Bytecode VM** | 默认栈式虚拟机引擎（纯计算加速约 70 倍，深度递归安全） | [Bytecode VM 引擎](#bytecode-vm-引擎) |
| **独立原生 Agent** | 无需 Cargo 的单一二进制打包（在线约 2.09MB，离线重放约 832KB） | [独立原生 Agent (无需 Cargo)](#独立原生-agent-无需-cargo) |
| **WebAssembly** | WASI 离线重放（约 526KB），在线宿主工具桥接 | [WebAssembly 目标](#webassembly-目标) |
| **确定性 Trace** | 开放 NTF v1 格式、$0 成本重放、CI 回归门禁、Trace 比较 | [确定性 Trace 与 Agent CI](#确定性-trace-与-agent-ci) |
| **隐私脱敏** | 核心卡口默认自动脱敏敏感秘钥与 PII 数据 | [隐私与 Trace 墨消脱敏](#隐私与-trace-墨消脱敏) |

## 语言与核心能力

### 控制流与数据

Nudge 提供开发者熟悉的结构化编程语法，并附带确定性安全边界：

```nudge
fn process_scores(raw: [int]) -> { total: int, count: int } {
    let mut total = 0
    let mut count = 0

    for (score, idx) in raw {
        if score >= 50 {
            total = total + score
            count = count + 1
        } else {
            // 忽略未通过的分数
        }
    }

    { total: total, count: count }
}
```

- **条件分支:** 支持多语句分支块的 `if/else` 与 `else if` 表达式。
- **循环迭代:** 顺序循环 `for item in coll` 与带索引循环 `for (item, idx) in coll`。
- **结构化数据:** 静态类型的记录字面量 `{ field1: expr1, field2: expr2 }` 及字段访问 (`obj.field`)。
- **索引访问:** 集合与记录的索引访问 (`items[0]`, `map["key"]`)。越界错误以 `E0705` 形式明确报错。
- **可变局部变量:** 显式变量声明 `let mut name = init` 及重新赋值 (`name = next`)。
- **循环次数上限:** 循环受 `NUDGE_MAX_LOOP_ITERATIONS`（默认 10,000）约束，防止无限循环失控，超限报错 `E0703`。
- **Optional 可选类型 (`T?`):** 使用 `?` 表示的可空类型（如 `string?`, `int?`, `(int | string)?`）。值缺失时为 `none`，使用 `== none` 与 `!= none` 进行判空检查。未经检查的非法使用将 fail-closed 报错 `E0201`。
- **Union 联合类型 (`T | U`):** 静态联合类型（如 `int | string` 或 `string | bool`），允许匹配其中的任一类型。支持编译期类型拓宽检查与严格的类型收窄拒绝 (`E0201`)。在 LLM 结构化输出模式、JSON 序列化和代码生成目标中均得到完整支持。

### 类型化 Effect

每个副作用都在函数签名中明确声明，并由编译器严格验证：

```nudge
fn run_pipeline() -> bool uses LLM, Tool, Workspace, Store {
    // 调用未声明的副作用将触发编译器错误 E0301
}
```

Nudge 定义了 10 种已知 Effect (`crates/nudgec/src/check.rs`):
- `LLM`: 调用大语言模型补全或流式输出。
- `Tool`: 调用外部 MCP 或注册工具。
- `IO`: 宿主输入输出操作 (`print`, `println`)。
- `Decision`: 通过 `decide{}` 查询决策模型。
- `Computer`: 通过 `computer.*` 与桌面环境及可访问性树交互。
- `Workspace`: 沙箱工作区内的作用域读写操作。
- `WorkspaceDelete`: 工作区内的破坏性文件或目录删除。
- `Store`: 通过 `store.*` 进行类型化持久键值存储操作。
- `UI`: 创建、挂载或分发图形界面事件。
- `Auth`: 访问经认证的提供商资源（支持作用域 `Auth(account)`）。

无 `uses` 子句的函数在编译期被证明为纯函数。未知的 Effect 将被编译器以 `E0302` 拒绝。

### 作用域 Workspace 与持久化 Store

本地智能体需要在不破坏系统安全的前提下操作文件并跨运行保持状态：

```nudge
record Config {
    theme: string,
    max_entries: int
}

fn update_workspace_file(path: string, new_content: string) -> bool uses Workspace, Store {
    // 1. 作用域文件系统访问 (禁止路径穿越与符号链接逃逸)
    if workspace.exists(path) {
        workspace.write(path, new_content)   // 原子写入: 临时暂存, fsync, 重命名
    }

    // 2. 类型化持久存储 (进程重启与系统重启后依然存在)
    store.set("app_config", Config { theme: "dark", max_entries: 100 })
    true
}
```

- **沙箱化 Workspace (`workspace.*`):** 所有文件操作均限制在显式工作区根目录中（通过 `--workspace` 或 `NUDGE_WORKSPACE_DIR` 配置，默认为当前目录）。拦截路径穿越 (`../`) 及指向工作区外部的符号链接。写操作具备崩溃一致性（先写临时文件后原子重命名）。
- **删除权限分离:** 删除文件或目录必须显式声明专用 `WorkspaceDelete` 副作用 (`uses WorkspaceDelete`)，防止常规写入例程误删目录。
- **持久化键值存储 (`store.*`):** 强类型数据存储保存于 `<workspace>/.nudge/store.json`（或 `NUDGE_STORE_PATH`）。与运行中临时检查点不同，此数据存储跨不同进程执行与重启持久保留。

### 进程控制与文件监控

构建驱动构建与测试运行器、语言服务器及实时重载工具链的智能体，同样遵循严格的副作用与授权纪律：

```nudge
fn watch_and_rebuild() -> bool uses FsWatch, Process {
    // 1. 作用域受限的有界文件监控 (杜绝无限挂起)
    let changes = fs.watch("src", 5000)          // 目录/路径 + 超时毫秒数

    // 2. 受控子进程: 命令程序与参数数组完全独立传递 (杜绝 shell 注入)
    let handle = proc.spawn("cargo", ["build", "--quiet"], 60000)
    let out = proc.await(handle)
    proc.alive(handle) == false
}
```

- **文件监控 (`fs.watch`, `FsWatch` 副作用):** 监控工作区路径的变更事件，受超时时间约束（永远不会无限期阻塞），严格限制在工作区根目录下。未声明的调用会被静态检查拦截 (`E0301`)。
- **受控子进程 (`proc.*`, `Process` 副作用):** `proc.spawn(cmd, argv, timeout_ms)` / `proc.run(...)` / `proc.await` / `proc.kill` / `proc.alive`。命令名与参数数组分开传递，不经过 shell 执行，杜绝 shell 注入漏洞。进程生命周期受控 (`E0921`)，杜绝孤儿进程（任务退出或取消时自动回收子进程）。执行默认 fail-closed: 除非经 `NUDGE_PROCESS_GRANTS` 授权，否则默认全部拒绝 (`E0920`，工作区越界 `E0922`)。

### 本地 UI

在 Nudge 中直接构建原生人机协同界面、审批工作台与交互式审查面板：

```nudge
type ReviewState = { prompt: string, approved: bool }

view ReviewCard(state: ReviewState) -> View {
    column(padding = 16, gap = 12) [
        text("Agent Review Station", size = 18, bold = true),
        text("Request: " + state.prompt),
        row(gap = 8) [
            button("Approve", on_click = approve_action),
            button("Reject", on_click = reject_action)
        ]
    ]
}

agent ReviewStation {
    state {
        prompt: string = "Execute deploy script",
        approved: bool = false
    }

    fn approve_action() uses UI, IO {
        state.approved = true
        io.println("Action approved")
    }

    fn reject_action() uses UI {
        state.approved = false
    }

    window MainWindow {
        title: "Nudge Review Station",
        width: 500,
        height: 300,
        content: ReviewCard(state)
    }
}
```

- **双模执行支持:**
  - **Headless 无头测试:** `mount_headless()` 与 `dispatch_click()` 完全在内存中对 UI 树与状态转换进行求值，无需打开真实窗口，在 CI 中以 $0 成本执行。
  - **原生桌面 GUI:** 在开启 `native-gui` Cargo 特性编译时，`window.open()` 将通过 `winit` 和 `softbuffer` 创建原生桌面窗口。
- **无头降级机制:** 设置 `NUDGE_UI_HEADLESS=1` 或在 Trace 重放模式 (`NUDGE_REPLAY`) 下，窗口调用将自动路由至 Headless 引擎。

### 任务生命周期与并发

通过正式的任务状态管理与协作式取消机制协调并发智能体分支：

```nudge
fn race_models(prompt: string) -> string uses LLM {
    // par race 自动取消竞态失败的分支
    par race {
        fast: query_fast_model(prompt),
        deep: query_deep_model(prompt)
    }
}
```

- **协作式取消:** 当竞态胜出判定后，失败分支会在循环、流和副作用边界处安全中断，避免在后台继续消耗 token 预算。
- **预算保护:** 取消任务后，`NUDGE_BUDGET` 下未使用的预算预留会立即释放。
- **权限快速闭锁:** 被取消的任务会立刻失去进一步调用工具 (`Tool`) 或计算机操作 (`Computer`) 的执行权限。
- **任务原语:** 提供 `task.spawn`, `task.join`, `task.status`, `task.cancel` 等显式任务管理能力。

### 提供商认证

深度集成于语言与运行时的安全凭证管理机制：

- **Fail-Closed 认证体系:** 函数通过 `uses Auth` 或 `uses Auth(account)` 显式声明鉴权依赖，未授权调用直接失败。
- **杜绝外部 Token 爬取:** Nudge 绝不读取第三方工具的私有 token 缓存或外部凭证库。
- **原生 RFC 8628 设备流:** 通过 OAuth 2.0 Device Authorization Grant 进行交互式终端授权，凭证安全存储于 `~/.nudge/credentials.json` (文件权限 0600)。
- **提供商安全门禁:** 提供商必须满足可验证的安全契约 (缺失凭证报错 `E-auth-0401`，作用域不足报错 `E-auth-0403`，禁用提供商报错 `E-auth-0404`)。
- **未验证授权的提供商保持关闭 (安全门禁):** 官方第三方使用授权未经证明的提供商默认 fail-closed。例如 `antigravity`: 虽然包含 OAuth 流程与实现，但由于尚未获得官方独立第三方调用许可，该门禁保持禁用 (`E-auth-0404`)。模拟测试或 CI 通过不能代表合规授权。经用户显式确认并承担风险后，可通过 `NUDGE_ENABLE_ANTIGRAVITY=1` 手动开启。

### 类型化决策

通过 `/v1/systemone` 协议编译来自决策模型（如 Laya 与 Jev 系列）的确定性：

```nudge
fn triage(t: string) -> string uses Decision {
    let d = decide {
        dept:    "which team handles this?" choose [billing, technical, security],
        churn:   "does the customer threaten to cancel?" yes/no,
        urgency: "how urgent is this?" score [low, soon, critical]
    }
    on t
    with { model: "laya:multilingual", deadline: 50 }   // 50ms 延迟预算
    route {
        auto: resolve(d.dept.winner) when d.dept.confidence > 0.8,
        review: escalate(d) otherwise
    }
}
```

- **批量并行求值:** 单次调用即可同时评估所有问题，返回胜出项、概率分布与置信度。
- **零成本测试:** 默认的仿真决策提供商合成确定性的伪随机分布，支持 $0 成本测试。
- **在线提供商接入:** 通过 `NUDGE_DECISION_SERVERS='{"laya": {"base_url": "http://localhost:8000"}}'` 接入本地或远程服务。
- **决策缓存:** `NUDGE_DECISION_CACHE=<path>` 跨运行缓存已验证的决策答案，杜绝重复计费。
- **策略扫描 (Policy Sweeps):** 运行 `nudgec policy-sweep trace.jsonl --question dept --thresholds 0.5,0.8` 在已录制分布上评估阈值调整，无需调用模型。详情阅读[决策文档](docs/decision.md)。

### 计算机操作 (Computer Use)

借助类型化的观察与动作掌控桌面环境，语言掌管控制与安全性，模型专职负责感知：

```nudge
fn automate_app(app: string) -> bool uses Computer, Decision {
    let obs = computer.observe(app, allow = [app], deadline = 30000)
    let check = decide {
        ready: "is the target button visible?" yes/no
    } on obs.tree
    with { deadline: 100 }

    let r = route {
        click: computer.click(1, allow = [app]) when check.ready.p > 0.5,
        close: computer.key("Escape", allow = [app]) otherwise
    }
    r.ok
}
```

- **类型化原语:** `computer.observe` 返回 `Observation` 记录（可访问性树、UI 元素、可选屏幕截图）。操作动作 (`click`, `type`, `key`, `scroll`, `set_value`, `drag`, `paste`) 返回包含调度回执的 `ActionResult`。
- **重放安全性:** Trace 重放绝不会重复触发物理操作。开启 `NUDGE_COMPUTER_DRIFT=1` 时，运行时将在线重新观察并计算机械漂移 (`Drift`) 差异，而不实际派发点击或按键。
- **安全加固:** 应用白名单控制 (`allow: [app]`)、控制器租约锁、过期元素索引拒绝 (`stale_state`)，以及硬性急停开关 (`NUDGE_COMPUTER_KILL=1`)。
- **确定性桌面测试:** 默认仿真提供商在内存中模拟 UI 场景 (`NUDGE_COMPUTER_SCENARIO`)，实现 $0 端到端测试。详情阅读[计算机操作文档](docs/computer-use.md)。

## 原生底层架构

### Bytecode VM 引擎

纯函数、数据转换与路径分支在专用栈式字节码虚拟机上执行 (`crates/nudgec/src/bytecode.rs` 与 `vm.rs`):

- **超高纯计算性能:** 纯计算代码执行速度比 AST 树遍历解释器快约 70 倍。
- **深度递归安全:** 托管调用栈帧有效防止递归算法引发操作系统栈溢出崩溃。
- **透明副作用委托:** 带有副作用的表达式 (`llm`, `tool`, `computer`, `workspace`) 通过 `DelegateCall` 和 `DelegateExpr` 从虚拟机无缝委托至运行时求值器。
- **默认执行引擎:** `nudgec run` 与独立二进制默认由字节码虚拟机驱动。设置 `NUDGE_VM=0` 可回退到 AST 解释器。

### 独立原生 Agent (无需 Cargo)

将你的 Agent 打包为单一独立的自包含原生可执行文件，构建现场无需任何开发工具链依赖：

```sh
# 将 Agent 打包为独立二进制文件
nudgec build --standalone my-agent.ndg -o bin/my-agent

# 在宿主环境直接运行 (无需 Python、Node.js 或 Rust 环境)
./bin/my-agent
```

- **AST 印章技术:** `nudgec` 对源码进行类型检查，将 AST 包序列化为 bincode，并在数毫秒内追加到预编译的 `nudge-agent` 运行时二进制尾部（以 `NDGAGNT1` 标识结尾）。
- **两种运行时规格:**
  - **Full 完整版（约 2.09MB，默认）:** 支持在线调用的运行时，内嵌基于 `ureq` + `rustls` 的 HTTP/TLS 协议栈，可直接连接实时 LLM 与 MCP 服务器。
  - **Replay 离线版（约 832KB）:** 精简运行时，剔除网络协议栈，所有操作均从录制的 NTF 跟踪中读取，$0 成本离线运行。
- **运行时自动定位:** 优先通过 `NUDGE_AGENT_RUNTIME` 环境变量指定，或自动查找与 `nudgec` 同级的运行时文件。详情阅读[独立打包文档](docs/standalone.md)。

### WebAssembly 目标

将智能体部署至浏览器沙箱、边缘运行时（如 Cloudflare Workers、Fastly）以及 WASI 环境：

- **WASI 离线重放 (`nudge-replay-wasi`):** 在约 526KB 的 `.wasm` 模块中通过 `wasmtime` 离线重放 NTF 跟踪，输出结果与原生运行完全逐字节一致：
  ```sh
  wasmtime run --dir . nudge-replay-wasi.wasm bundle.json trace.jsonl
  ```
- **在线宿主工具桥接:** 运行于浏览器或边缘 JavaScript 环境的 Agent 可通过标准化的 `nudge_host_tool_call` 导入契约发起真实工具调用（已在 Node WASI 环境验证，见 `examples/wasm-host/`）。
- **沙箱安全保障:** 在 `wasm32` 目标上严格隔离网络 socket 与操作系统进程派发，确保安全受限运行。详情阅读[WebAssembly 文档](docs/wasm.md)。

## 工具链与生态集成

### MCP 集成

声明通过标准输入输出 (stdio) 与 Model Context Protocol (MCP) 服务器交互的工具：

```nudge
type SearchResult = { url: string, title: string }

tool web_search(query: string) -> [SearchResult] {
    impl: mcp("search").web_search(query)
    side_effects: none
}

fn main() -> [SearchResult] uses Tool {
    web_search("nudge language")
}
```

通过 `NUDGE_MCP_SERVERS` 配置在线服务器：

```sh
export NUDGE_MCP_SERVERS='{"search": {"command": "mcp-server-websearch"}}'
nudgec run search_agent.ndg
```

参数以具名 JSON 参数对象传递，完全兼容 FastMCP 与标准 MCP 服务器。服务不可用时快速失败并给出清晰诊断。

### 确定性 Trace 与 Agent CI

每次运行均会输出符合开放 NTF v1 规范的 JSONL 跟踪文件，记录所有副作用交互：

```sh
# 验证 trace 跟踪文件是否符合公开标准规范
nudgec trace-check trace.jsonl

# 比较两份 trace 跟踪文件以发现成本或行为变更 (--json 输出机器可读报告)
nudgec trace-diff baseline.jsonl candidate.jsonl --fail-on-regression --json
```

**GitHub Action (`action.yml`):** 每次代码 push 时在 CI 中以 $0 成本测试智能体，无需配置任何 API 密钥：

```yaml
name: agents
on: [push, pull_request]
jobs:
  test-agents:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: NekomyaDev/nudge@v1
        with:
          files: "agents/**/*.ndg"
          trace-diff: "baseline.jsonl candidate.jsonl"
```

冻结的 NTF 规范位于 [docs/ntf-spec.md](docs/ntf-spec.md)，包含完整测试套件 [`conformance/`](conformance/) 以及针对 LangChain、CrewAI 与 LangGraph 的适配桥接器。

### 开发者工具

- **静态成本估算器:** `nudgec cost <file.ndg> [--json]` 在编译期计算各函数的最坏情况 token 消耗与预算边界。
- **属性与 Fuzz 测试:** `nudgec fuzz [file.ndg] [--iterations N] [--seed S] [--json]` 执行基于变异的模糊测试与不变量属性验证。
- **数据集评估评测:** `nudgec eval <file.ndg> --dataset <rows.jsonl> [--fn <name>] [--path <dotted>] [--min-accuracy 0.8] [--json]` 针对 JSONL 数据集评估测试智能体函数，并支持按准确率门禁阻断 CI。
- **评测对比工具:** `nudgec compare <results_a.jsonl> <results_b.jsonl> [--json]` 比较两轮数据集评测结果，快速发现准确率或性能回归。
- **运行解释器:** `nudgec explain <trace.jsonl> [--json]` 输出人类可读的终端摘要或结构化 JSON 分析，突出失败、token 消耗与低置信度决策。
- **Trace 差异对比:** `nudgec trace-diff <a.jsonl> <b.jsonl> [--json] [--fail-on-regression]` 对比两份运行 trace 以检测行为回归，开启门禁时遇回归以状态码 1 退出。
- **合规审计分析:** `nudgec audit <trace.jsonl> [--json]` 审计 NTF trace 是否符合欧盟人工智能法案 (EU AI Act) 合规要求（透明度、可治理性、未脱敏秘钥检测、风险阈值）。
- **本地 Trace 查看器:** `nudgec trace-view <trace.jsonl> [--port N] [--no-open]` 启动本地 Web UI，可视化展示执行时间线、token 支出与自我修复步骤。
- **代码格式化与监控:** `nudgec fmt <file.ndg> [--check] [--json]` 安全格式化代码，`nudgec check <file.ndg> [--json] --watch` 在每次保存时自动进行类型检查。
- **代码检查与自动修复:** `nudgec lint <file.ndg> [--fix] [--json]` 检查最佳实践（未设定预算、模糊提示词、缺少重试修复）并自动应用安全修复。
- **运行存储检查器:** `nudgec runs [--run <run_id>] [--json]` 检查持久化智能体运行状态、追踪记录与检查点。
- **身份验证管理:** `nudgec auth login [provider]` 与 `nudgec auth status [--json]` 管理 OAuth 凭据并验证已连接的 AI 提供商。
- **静态 HTML 导出:** `nudgec trace-html <trace.jsonl> [-o report.html]` 导出完全自包含的交互式审计报表。

### VS Code 扩展

安装 [Nudge Language Extension](https://marketplace.visualstudio.com/items?itemName=Nekomya.nudge-lang) 获得语法高亮、代码片段、悬浮文档支持，以及由 `nudgec lsp` 驱动的实时类型诊断。

## 隐私与 Trace 墨消脱敏

AI 智能体经常处理敏感的 token、鉴权头、私钥以及用户个人身份信息 (PII)。Nudge 通过 fail-closed 的 trace 脱敏流水线守护你的数据隐私：

- **单点核心卡口:** 脱敏在记录被序列化写入磁盘或内存缓冲区之前的 `TraceWriter::emit` 处统一完成。所有记录类型 (`llm.call`, `tool.call`, `decision.call`, `computer.*`, `fn.return`) 均在这一关口统一遮蔽。
- **默认 Fail-Closed 开启:** Trace 敏感信息脱敏默认处于激活状态 (`NUDGE_TRACE_REDACT=1`)。仅在显式本地调试时设置 `NUDGE_TRACE_REDACT=0`。
- **`NUDGE_TRACE_REDACT=0` 是安全边界而非便利开关:** 关闭脱敏将导致秘钥、API Key、认证头以及个人数据以**明文**形式直接写入 trace 文件。这是该关闭开关经测试验证的既定行为，绝不可在 CI、公开提交的产物或基于 Nudge 的应用日志收集环境中使用。下游消费者必须将 `NUDGE_TRACE_REDACT=0` 的存在视为安全警告标志。
- **自动检测秘钥模式:** 自动识别并屏蔽以 `_KEY`, `_SECRET`, `_TOKEN`, `_PASSWORD`, `_PAT` 结尾或包含 `AUTH` 的环境变量值（长度达到 6 位及以上）。
- **多层正则模式遮蔽:** 强力过滤 Bearer token、认证请求头、键值凭据对、私钥块以及各厂商 token 前缀（OpenAI `sk-...`, Anthropic `sk-ant-...`, GitHub `ghp_...`, AWS `AKIA...`, Slack `xox...`）。
- **密码学重放不变性:** 基于内容寻址的 `request_hash` 是基于**脱敏前**的原始输入计算的。这确保了脱敏后的 trace 在离线重放时依然能实现 O(1) 逐字节精确命中，且无需将任何敏感凭证持久化至磁盘。

## 环境变量

| 变量名 | 生效范围 | 用途说明 | 默认值 |
|:---|:---|:---|:---|
| `NUDGE_PROVIDER` | 全部 | 当前 LLM 提供商 (`fake`, `openai`, `anthropic`, `gemini`, `groq`, `ollama`) | `fake` |
| `NUDGE_VM` | 原生 | 字节码虚拟机执行引擎开关 (`1` 为默认开启 VM, `0` 回退至 AST 解释器) | `1` |
| `NUDGE_DEFAULT_MODEL` | 原生 | 当 `with` 块未显式指定模型时的默认模型标识 | 提供商默认 |
| `NUDGE_API_KEY` / `NUDGE_BASE_URL` | 全部 | OpenAI 兼容接口的凭证密钥与基础请求端点 | 无 |
| `NUDGE_PRICING` | 全部 | 自定义定价 JSON 映射表: `{"model": [usd_per_1M_in, usd_per_1M_out]}` | 内置定价表 |
| `NUDGE_GUARD` | Python/TS | 输出安全守卫 (`pii` 自动过滤 LLM 输出中的秘钥、邮箱、IP) | 无 |
| `NUDGE_BUDGET` | 全部 | 单次运行的全局 USD 花费上限，跨并发分支严格执行 | 无 |
| `NUDGE_REPAIR_BUDGET` | Python | 修复重试轮次的累计 USD 支出上限 | 无 |
| `NUDGE_MAX_LOOP_ITERATIONS` | 原生 | 循环顺序执行的最大迭代次数，超限报错 `E0703` | `10000` |
| `NUDGE_TRACE` | 全部 | 输出 NTF JSONL 运行跟踪文件的目标路径 | `trace.jsonl` |
| `NUDGE_REPLAY` | 全部 | 确定性离线重放跟踪文件路径 (`NUDGE_REPLAY_MODE=all` 或 `llm`) | 无 |
| `NUDGE_RESUME` | 全部 | 崩溃后基于已录制的 trace 前缀从最后一个检查点恢复执行 | 无 |
| `NUDGE_TRACE_REDACT` | 原生 | 开启单点卡口的密钥与 PII 墨消脱敏 (`1` 或 `0`) | `1` (激活) |
| `NUDGE_TRACE_REDACT_ENV` | 原生 | 自动脱敏 trace 中出现的敏感环境变量值 | `1` (激活) |
| `NUDGE_TRACE_REDACT_KEYS` | 原生 | 逗号分隔的附加待脱敏 JSON 对象键名列表 | 无 |
| `NUDGE_TRACE_REDACT_PATTERNS` | 原生 | 逗号分隔的自定义正则列表或正则规则文件路径 | 无 |
| `NUDGE_WORKSPACE_DIR` | 原生 | 沙箱 `workspace.*` 操作的根目录权限范围 | 当前目录 |
| `NUDGE_STORE_PATH` | 原生 | 类型化持久键值数据的保存文件路径 | `<workspace>/.nudge/store.json` |
| `NUDGE_UI_HEADLESS` | 原生 | 强制 UI 窗口操作在内存中无头静默运行 (`1` 或 `true`) | `0` |
| `NUDGE_TOOL_GRANTS` | 全部 | 执行层 JSON 工具授权策略 (例如 `{"web_search": ["*"]}`)，默认 fail-closed | 无 |
| `NUDGE_PROCESS_GRANTS` | 原生 | `proc.*` 命令和参数的 Fail-closed JSON 执行授权策略 | 无 (默认拒绝) |
| `NUDGE_PROCESS_TIMEOUT` | 原生 | `proc.*` 派发子进程的默认执行超时毫秒数 | `30000` |
| `NUDGE_FS_WATCH_TIMEOUT` | 原生 | `fs.watch` 轮询和等待的默认超时毫秒数 | `30000` |
| `NUDGE_ENABLE_ANTIGRAVITY` | 原生 | 激活 Antigravity 提供商的手动授权确认开关 (`1` 或 `true`) | `0` |
| `NUDGE_AUTH_ANTIGRAVITY` | 原生 | Antigravity 提供商的直传 Bearer 认证 Token | 无 |
| `NUDGE_AUTH_GITHUB_COPILOT` | 原生 | GitHub Copilot 提供商的直传 Token | 无 |
| `NUDGE_MCP_SERVERS` | 全部 | 映射 MCP 服务器标识与 stdio 执行命令的 JSON 注册表 | 无 |
| `NUDGE_MCP_TIMEOUT_MS` | 原生 | MCP 服务器单次请求读取与挂起超时时间毫秒数 | `30000` |
| `NUDGE_DECISION_SERVERS` | 全部 | 决策模型接口注册表 (例如 `{"laya": {"base_url": "..."}}`) | 无 |
| `NUDGE_DECISION_CACHE` | 全部 | 持久化决策模型概率分布的缓存文件路径 | 无 |
| `NUDGE_COMPUTER_SERVERS` | 全部 | 计算机操作桥接服务注册表 (子进程 JSONL 或 HTTP) | 无 |
| `NUDGE_COMPUTER_PROVIDER` | 全部 | 计算机操作的桥接提供商 (`fake`, 或注册的服务器名) | `fake` |
| `NUDGE_COMPUTER_KILL` | 全部 | 硬性终止开关: `1` 立即拒绝所有计算机操作调用 | `0` |
| `NUDGE_COMPUTER_DRIFT` | 全部 | 为 `1` 时，重放期间重新在线观察并输出机械漂移差异 | `0` |
| `NUDGE_AGENT_RUNTIME` | 原生 | `nudgec build --standalone` 使用的预编译 `nudge-agent` 路径 | 同级二进制 |
| `NUDGE_CREDENTIALS_PATH` | 原生 | Nudge 认证凭证存储的自定义文件路径 | `~/.nudge/credentials.json` |
| `NUDGE_OTEL` | Python | OpenTelemetry span 导出文件路径 | 无 |
| `NUDGE_OTEL_ENDPOINT` | Python | OpenTelemetry HTTP 导出目标端点 | 无 |

`原生` 适用于 Rust 运行时 (`nudgec`/`nudge-runtime`)，`Python/TS` 适用于生成的代码运行时，`全部` 适用于两者。

## 真实应用示例

在 [`examples/`](examples/) 目录探索完整的智能体实现：

| 示例 | 应用场景 | 核心亮点 |
|:---|:---|:---|
| [AI 聊天机器人](examples/chatbot/) | 对话交互助理 | 类型化记忆模式、确定性重放、预算硬约束 |
| [工单分拣分类器](examples/triage-agent/) | 客户服务分类 | 批量 `decide{}` 决策块、置信度阈值、路由策略 |
| [代码审查器](examples/code-reviewer/) | 软件工程质检 | 结构化语法诊断、自动化评分规则 |
| [深度研究智能体](examples/research_agent.ndg) | 多源信息综合 | `par map` 并行检索、置信度评分 |
| [桌面自动化](examples/computer-automation/) | 桌面操作智能体 | 屏幕观察、可访问性树分析、漂移验证 |
| [RAG 检索增强智能体](examples/rag-agent/) | 知识库检索与生成 | MCP stdio 检索工具绑定、事实依据引用抽取 |
| [属性 Fuzz 测试](examples/property-fuzz/) | 智能体安全测试 | `for_all` 不变量测试、提示词注入模糊测试 |
| [地牢探险游戏](examples/dungeon-crawler/) | 状态机模拟演练 | 纯 Nudge 编写、Agent 状态迁移、回合制战斗 |

## 许可证与源码构建

Nudge 采用 **Apache-2.0** 开源许可证。详见 [LICENSE](LICENSE)。

### 源码构建

构建编译器需要 Rust 1.75+ (stable):

```sh
git clone https://github.com/NekomyaDev/nudge.git
cd nudge
cargo build --release -p nudgec   # 编译器二进制位于 target/release/nudgec
cargo test --workspace            # 运行编译器、VM 及运行时完整测试套件
```

Python 运行时位于 `runtime/` 目录（仅依赖标准库，零外部第三方依赖）:

```sh
pip install ./runtime
```

---

<p align="center">
  Made with love by <a href="https://github.com/NekomyaDev">NekomyaDev</a>
</p>
