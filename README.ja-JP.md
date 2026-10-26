<p align="center">
  <img src="assets/logo.svg" width="200" height="200" alt="Nudge Logo">
</p>

<h1 align="center">Nudge</h1>

<p align="center">
  <strong>エージェントをパースするな、ナッジ（誘導）せよ。</strong><br>
  AIエージェントのための、型付き・リプレイ可能・予算認識プログラミング言語。<br>
  ネイティブBytecodeランタイムおよびスタンドアロンコンパイラ。PythonおよびTypeScriptへコンパイル可能。
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

## なぜ Nudge なのか？

本番環境のエージェントは今なおグルーコードでつなぎ合わされています。手作業でパースされるプロンプトチェーン、try/exceptブロックで囲まれたツール呼び出し、欠落したリプレイ機能、予測できないコスト、そして存在しない回帰テスト。ライブラリは問題が起きた後に症状をパッチするだけです。**Nudge は問題が実際に存在するレイヤー、すなわちプログラミング言語そのものを修正します。**

| 課題 | 既存ライブラリ | Nudge |
|:---|:---|:---|
| 型のないLLM出力 | 実行時にバリデーション | スキーマは第一級の言語型であり、コンパイル時に検証 |
| 隠れた副作用 | 不可視 | シグネチャで明示的に宣言される10種類の型付きEffect (`uses LLM, Tool, IO, ...`) |
| 回帰テストの欠如 | 記録とリプレイを後付け | すべての実行がNTFトレースを出力し、すべてのトレースは決定論的な$0テスト |
| 予期せぬコスト | 請求書が届いた後に監視 | 予算（Budget）はコンパイラとランタイムが強制する厳格な規約 |
| 並行処理のスパゲッティ | 手動の非同期ループ | コンパイル時のレース安全性を備えたネイティブな `par map / race / all` |

## Nudge を体験する

```nudge
type Finding = { claim: string, source: Url, confidence: float @range(0, 1) }

fn analyze(q: string, hits: [SearchResult]) -> [Finding] uses LLM {
    llm"""Extract verifiable findings about {q} from: {hits}"""
    with { schema: [Finding], model: "anthropic:sonnet-4.6",
           budget: 0.03 USD, retry: 2 with repair }
}

test "stays within budget on recorded trace" {
    let t = replay("traces/demo.jsonl")
    assert t.cost_usd < 0.25          // zero tokens burned in CI
}
```

コンパイラはスキーマの一致を証明し、Effectを推論し、最悪ケースのコストを上限値として拘束します。ランタイムはすべての呼び出しをコンテンツアドレス指定されたトレースに記録するため、diffの取得、コミット、そして$0でのリプレイが可能です。

## クイックスタート

### 1. Nudge のインストール

**ワンラインインストーラ（推奨）：**

```sh
# Linux / macOS
curl -fsSL https://raw.githubusercontent.com/NekomyaDev/nudge/main/install.sh | bash

# Windows (PowerShell を管理者として実行)
irm https://raw.githubusercontent.com/NekomyaDev/nudge/main/install.ps1 | iex
```

**パッケージマネージャとコンテナ：**

```sh
# Snap (Linux)
sudo snap install nudge --classic

# Docker
docker run -it --rm -v $(pwd):/workspace nekomyadev/nudge nudgec --help
```

Linux、macOS、Windows 向けのビルド済みバイナリも [Releases](https://github.com/NekomyaDev/nudge/releases) ページから入手可能です。

### 2. 60秒の初回実行（ネイティブ、$0）

Python、Node.js、APIキーは一切不要です。Nudge にはネイティブスタックVMと、インメモリの決定論的 fake プロバイダーが含まれています：

```sh
# プログラムを作成
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

# プログラムの型検査
nudgec check hello.ndg

# ネイティブBytecode VM上で直接実行（$0、キー不要）
nudgec run hello.ndg

# オフライン回帰テストを実行
nudgec test hello.ndg
```

### 3. インタラクティブレッスンとプロジェクト作成

```sh
# ターミナル上で動作する6つのインタラクティブレッスン (hello, types, decide, policy, test, trace)
nudgec learn

# 用意されたテンプレートからプロジェクトをスキャフォールド (triage, chatbot, rag-agent など)
nudgec init my-agent --template triage
cd my-agent && nudgec check my-agent.ndg && nudgec run my-agent.ndg
```

### 4. Python または TypeScript へのコンパイル（オプション）

既存の Python または TypeScript サービスへデプロイする場合：

```sh
# Python ターゲット
pip install nudge-runtime          # standard library only, zero dependencies
nudgec build hello.ndg              # emits out/hello.py
python3 out/hello.py

# TypeScript ターゲット
nudgec build-ts hello.ndg             # emits out/hello.ts
```

## コア機能一覧

| 機能 | 提供される価値 | セクション |
|:---|:---|:---|
| **型付きLLM呼び出し** | 出力スキーマが型として機能し、違反時は自動修復をトリガー | [Nudge を体験する](#nudge-を体験する) |
| **制御フローと型システム** | `if/else`、`for` ループ、レコードリテラル `{k: v}`、インデックス参照、ミュータブルローカル変数、Optional (`T?`)、Union (`T \| U`) | [制御フローとデータ](#制御フローとデータ) |
| **型付きEffect** | コンパイル時に追跡・検査される10種類の既知のEffect | [型付きEffect](#型付きeffect) |
| **ワークスペースとストア** | サンドボックス化されたファイルシステム + 型付き永続Key-Valueストア | [スコープ付きワークスペースと永続ストア](#スコープ付きワークスペースと永続ストア) |
| **ローカルUI** | 型付きView、ウィンドウ、ヘッドレステスト、OSネイティブデスクトップウィンドウ | [ローカルUI](#ローカルui) |
| **タスクと並行性** | タスクのspawn/join、協調的キャンセル、安全な `par race`、予算の解放 | [タスクライフサイクルと並行性](#タスクライフサイクルと並行性) |
| **プロバイダー認証** | Fail-Closed（安全側に倒す）RFC 8628 OAuthデバイスフロー、第三者ツールのトークン取得なし | [プロバイダー認証](#プロバイダー認証) |
| **型付き判定** | バッチ分類、Yes/No判定、状態に対するルーブリックスコアリング (`/v1/systemone`) | [型付き判定](#型付き判定) |
| **コンピュータ操作** | 画面観測、型付きアクション、アプリ許可リスト、キルスイッチ | [コンピュータ操作](#コンピュータ操作) |
| **Bytecode VM** | デフォルトのスタックVMエンジン（純粋計算で約70倍高速、深い再帰の安全性） | [Bytecode VM エンジン](#bytecode-vm-エンジン) |
| **スタンドアロンエージェント** | Cargo不要の単一バイナリパッケージング（ライブ実行時約2.09MB、リプレイ専用時約832KB） | [スタンドアロンネイティブエージェント（Cargo不要）](#スタンドアロンネイティブエージェントcargo不要) |
| **WebAssembly** | WASI環境でのオフラインリプレイ（約526KB）、ライブツールホストブリッジ | [WebAssembly ターゲット](#webassembly-ターゲット) |
| **決定論的トレース** | オープンNTF v1トレース、$0リプレイ、CI回帰ゲート、トレース比較（diff） | [決定論的トレースとエージェントCI](#決定論的トレースとエージェントci) |
| **プライバシー墨消し** | 単一のチョークポイントによるシークレット/PIIのデフォルト自動墨消し | [プライバシーとトレース墨消し（リダクション）](#プライバシーとトレース墨消しリダクション) |

## 言語仕様と機能

### 制御フローとデータ

Nudge は、決定論的な安全境界を備えた馴染み深い構造化プログラミング構文を提供します：

```nudge
fn process_scores(raw: [int]) -> { total: int, count: int } {
    let mut total = 0
    let mut count = 0

    for (score, idx) in raw {
        if score >= 50 {
            total = total + score
            count = count + 1
        } else {
            // skip failing scores
        }
    }

    { total: total, count: count }
}
```

- **条件分岐:** 複数文のブロックをサポートする `if/else` および `else if` 式。
- **反復処理:** 順次実行する `for item in coll` ループ、およびインデックス付きの `for (item, idx) in coll` ループ。
- **構造化データ:** 静的型付けとフィールドアクセス (`obj.field`) を持つレコードリテラル `{ field1: expr1, field2: expr2 }`。
- **インデックス参照:** コレクションやレコードに対するインデックスアクセス (`items[0]`, `map["key"]`)。境界外エラーは `E0705` としてクリーンに失敗します。
- **ミュータブルローカル変数:** 明示的な `let mut name = init` 変数宣言と再代入 (`name = next`)。
- **ループ上限:** ループは暴走を防ぐために `NUDGE_MAX_LOOP_ITERATIONS`（デフォルト10,000）で制限されており、上限超過時は `E0703` を発生させます。
- **Optional 型 (`T?`):** `?` で表される null 許容型（`string?`、`int?`、`(int | string)?` など）。値が存在しない場合は `none` となり、`== none` および `!= none` で判定します。未チェックの使用は fail-closed で `E0201` エラーとなります。
- **Union 型 (`T | U`):** 指定した複数の型候補を受け入れる静的 Union 型（`int | string` や `string | bool` など）。コンパイル時の型の拡大（widening）チェックと厳密な型の絞り込み拒否（`E0201`）を強制します。LLM 出力スキーマ、JSON 直列化、コード生成ターゲットで完全サポートされています。

### 型付きEffect

すべての副作用は関数シグネチャで宣言され、コンパイラによって検証されます：

```nudge
fn run_pipeline() -> bool uses LLM, Tool, Workspace, Store {
    // Calling unlisted effects triggers compiler error E0301
}
```

Nudge は10種類の既知のEffectを定義しています (`crates/nudgec/src/check.rs`):
- `LLM`: 言語モデルの補完またはストリーミングプロンプトの呼び出し。
- `Tool`: 外部MCPツールまたは登録ツールの呼び出し。
- `IO`: ホスト入出力操作 (`print`, `println`)。
- `Decision`: `decide{}` を介した判定モデルのクエリ。
- `Computer`: `computer.*` を介したデスクトップ環境およびアクセシビリティツリーとのやり取り。
- `Workspace`: サンドボックス化されたワークスペース内でのスコープ付き読み書き操作。
- `WorkspaceDelete`: ワークスペース内での破壊的なファイルまたはディレクトリの削除。
- `Store`: `store.*` を介した永続Key-Valueストレージ操作。
- `UI`: グラフィカルUI要素の作成、マウント、またはイベントディスパッチ。
- `Auth`: 認証済みプロバイダーリソースへのアクセス（スコープ付き `Auth(account)` をサポート）。

`uses` 節のない関数はコンパイル時に純粋関数であることが証明されます。未知のEffectは `E0302` で拒否されます。

### スコープ付きワークスペースと永続ストア

ローカルエージェントはシステムのセキュリティを損なうことなく、ファイルを操作し、実行をまたいで状態を維持する必要があります：

```nudge
record Config {
    theme: string,
    max_entries: int
}

fn update_workspace_file(path: string, new_content: string) -> bool uses Workspace, Store {
    // 1. Scoped filesystem access (path traversal and symlink escapes denied)
    if workspace.exists(path) {
        workspace.write(path, new_content)   // Atomic write: staging, fsync, rename
    }

    // 2. Typed persistent store (survives process restarts and reboots)
    store.set("app_config", Config { theme: "dark", max_entries: 100 })
    true
}
```

- **サンドボックス化ワークスペース (`workspace.*`):** すべての操作は明示的なワークスペースルート（`--workspace` または `NUDGE_WORKSPACE_DIR` で設定、デフォルトはカレントディレクトリ）内に制限されます。パストラバーサル (`../`) やルート外へのシンボリックリンク脱出はブロックされます。書き込みはクラッシュ一貫性を保持します（一時ファイルへのステージング、fsync、リネームによるアトミック書き込み）。
- **削除権限の分離:** ファイルまたはディレクトリの削除には専用の `WorkspaceDelete` Effect (`uses WorkspaceDelete`) が必要であり、通常の書き込みルーチンがディレクトリを削除することを防止します。
- **型付き永続Key-Valueストア (`store.*`):** `<workspace>/.nudge/store.json`（または `NUDGE_STORE_PATH`）に保存される強い型付けのストレージです。一時的な実行チェックポイントとは異なり、このデータストアは実行プロセスの再起動やOSの再起動をまたいで永続化されます。

### プロセスとファイル監視

ビルド/テストランナー、言語サーバー、ライブリロードツールを駆動するエージェントを、他のすべてと同じEffectと権限の規律のもとで構築できます：

```nudge
fn watch_and_rebuild() -> bool uses FsWatch, Process {
    // 1. Bounded, workspace-confined file watching (no indefinite hang)
    let changes = fs.watch("src", 5000)          // dir/path + timeout (ms)

    // 2. Controlled child process: program and args SEPARATELY (never a shell)
    let handle = proc.spawn("cargo", ["build", "--quiet"], 60000)
    let out = proc.await(handle)
    proc.alive(handle) == false
}
```

- **ファイル監視 (`fs.watch`, `FsWatch` Effect):** ワークスペースパスの変更イベントを監視します。タイムアウトで境界付けされ（無期限にハングしません）、ワークスペースルート内に限定されます（外部への脱出は拒否）。未宣言の使用はコンパイル時に拒否されます (`E0301`)。
- **制御された子プロセス (`proc.*`, `Process` Effect):** `proc.spawn(cmd, argv, timeout_ms)` / `proc.run(...)` (spawn + await) / `proc.await` / `proc.kill` / `proc.alive`。`cmd` とその `argv` は**別々に**渡され、シェルを経由せずに実行されるため、シェルインジェクションの脆弱性がありません。ライフサイクルは境界付けされており (`E0921`)、タスク終了時やキャンセル時には子プロセスが回収され、キャンセル時強制終了により**孤児プロセスは発生しません**。実行はデフォルトで**Fail-Closed（遮断）**となっており、`NUDGE_PROCESS_GRANTS` で許可されていない限り拒否されます (`E0920`、ワークスペース制限 `E0922`)。

### ローカルUI

ネイティブなHuman-in-the-loopインターフェース、承認ステーション、インタラクティブ検査パネルを Nudge 上に直接構築できます：

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

- **デュアルモード実行:**
  - **ヘッドレステスト:** `mount_headless()` と `dispatch_click()` により、実際のウィンドウを開かずにメモリ内ですべてのUIツリーと状態遷移を評価します。CI上で$0で動作します。
  - **ネイティブデスクトップGUI:** Cargo の `native-gui` フィーチャー付きでビルドされた場合、`window.open()` は `winit` と `softbuffer` を用いてOSネイティブウィンドウを生成します。
- **ヘッドレスフォールバック:** `NUDGE_UI_HEADLESS=1` の設定時やトレースリプレイ中 (`NUDGE_REPLAY`) は、ウィンドウ関連の呼び出しが自動的にヘッドレスエンジンにルーティングされます。

### タスクライフサイクルと並行性

正式なタスク状態管理と協調的キャンセルにより、並行して動作するエージェントのアームを協調させます：

```nudge
fn race_models(prompt: string) -> string uses LLM {
    // par race automatically cancels losing branches
    par race {
        fast: query_fast_model(prompt),
        deep: query_deep_model(prompt)
    }
}
```

- **協調的キャンセル:** レースが解決すると、敗北したブランチはループ境界、ストリーム境界、Effect境界で停止し、バックグラウンドで予算を消費し続けるのを防ぎます。
- **予算保護:** `NUDGE_BUDGET` のもとで予約された未消費の予算枠は、タスクのキャンセル時に即座に解放されます。
- **Fail-Closed な権限:** キャンセルされたタスクは、それ以降のツール呼び出し (`Tool`) やコンピュータ操作 (`Computer`) のディスパッチ権限を即座に失います。
- **タスクプリミティブ:** `task.spawn`、`task.join`、`task.status`、`task.cancel` による明示的なタスク制御。

### プロバイダー認証

言語およびランタイムに直接組み込まれた安全な認証情報管理：

- **Fail-Closed 認証:** 関数は `uses Auth` または `uses Auth(account)` を用いて明示的な認証要件を宣言します。未認証の呼び出しは安全側に倒して遮断されます。
- **第三者ツールのトークン取得なし:** Nudge はサードパーティツールのプライベートなトークンキャッシュや認証情報ストアを読み取ることは決してありません。
- **ネイティブ RFC 8628 デバイスフロー:** OAuth 2.0 Device Authorization Grant によるターミナル上での対話型認証と、`~/.nudge/credentials.json` への安全な保存（ファイルパーミッション 0600）。
- **プロバイダーゲート:** プロバイダーは検証可能なセキュリティ規約を満たす必要があります（認証情報欠落は `E-auth-0401`、スコープ不足は `E-auth-0403`、無効化プロバイダーは `E-auth-0404`）。
- **未検証プロバイダーの遮断（権限ゲート）:** 公式な第三者利用権限が証明されていないプロバイダーは、**デフォルトで Fail-Closed（無効化）**となります。これは現在 `antigravity` に適用されています。OAuthログインフローとプロバイダーコードは存在しますが、そのOAuthクライアントとモデルエンドポイントを独立したサードパーティクライアントから使用できる公式な証拠が見つかっていないため、権限が検証されるまでゲートは**無効のまま** (`E-auth-0404`) となります。**モックOAuthテストやCIのパスは、実際のアカウントアクセスや利用権限を証明するものではありません。** 利用者（Nudge上に構築されたあらゆるアプリケーションを含む）は、権限が文書化されるまでこの接続に依存してはなりません。権限確認後、明示的なオプトイン (`NUDGE_ENABLE_ANTIGRAVITY=1`) によりユーザー自身の責任で有効化できます。

### 型付き判定

`/v1/systemone` ワイヤプロトコルを通じて、判定モデル（Laya や Jev など）からの不確実性をコンパイルします：

```nudge
fn triage(t: string) -> string uses Decision {
    let d = decide {
        dept:    "which team handles this?" choose [billing, technical, security],
        churn:   "does the customer threaten to cancel?" yes/no,
        urgency: "how urgent is this?" score [low, soon, critical]
    }
    on t
    with { model: "laya:multilingual", deadline: 50 }   // ms latency budget
    route {
        auto: resolve(d.dept.winner) when d.dept.confidence > 0.8,
        review: escalate(d) otherwise
    }
}
```

- **バッチ実行:** 1回の呼び出しですべての質問を同時に評価し、勝者、確率、分布、信頼度を返します。
- **ゼロコストテスト:** デフォルトの fake 判定プロバイダーが決定論的なシード付き分布を合成するため、$0でのテスト実行が可能です。
- **ライブプロバイダー:** `NUDGE_DECISION_SERVERS='{"laya": {"base_url": "http://localhost:8000"}}'` を介してローカルまたはリモートのモデルに接続。
- **判定キャッシュ:** `NUDGE_DECISION_CACHE=<path>` により検証済みの回答を実行間でキャッシュし、同一状態に対して二重に支払うことを防ぎます。
- **ポリシー走査:** `nudgec policy-sweep trace.jsonl --question dept --thresholds 0.5,0.8` を実行して、記録済み分布に対するしきい値調整をモデル呼び出しゼロで評価可能。[判定ドキュメント](docs/decision.md) をご覧ください。

### コンピュータ操作

言語が制御と安全性を担保し、モデルが知覚を担当する、型付きの観測とアクションによるデスクトップ環境の制御：

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

- **型付きプリミティブ:** `computer.observe` は `Observation` レコード（アクセシビリティツリー、要素、任意のスクリーンショット）を返します。アクション (`click`, `type`, `key`, `scroll`, `set_value`, `drag`, `paste`) はディスパッチ受付証 (`action_sent`) を含む `ActionResult` を返します。
- **リプレイの安全性:** トレースリプレイが物理的なアクションを再実行することは決してありません。`NUDGE_COMPUTER_DRIFT=1` を指定すると、ランタイムはライブ状態を再観測し、アクションを実行することなく機械的な `Drift` diff を計算します。
- **堅牢化:** アプリ許可リスト (`allow: [app]`)、コントローラーリースロック、陳腐化した要素インデックスの拒否 (`stale_state`)、およびハードキルスイッチ (`NUDGE_COMPUTER_KILL=1`)。
- **決定論的デスクトップ:** デフォルトの fake プロバイダーがインメモリのUIシナリオ (`NUDGE_COMPUTER_SCENARIO`) を実行し、$0でのエンドツーエンドテストを可能にします。[コンピュータ操作ドキュメント](docs/computer-use.md) をご覧ください。

## ネイティブインフラストラクチャ

### Bytecode VM エンジン

純粋関数、データ変換、ルート分岐は専用のスタックベースBytecode VM (`crates/nudgec/src/bytecode.rs` および `vm.rs`) 上で実行されます：

- **高速な純粋計算:** ASTツリー探索インタープリタに比べ、純粋コードを約70倍高速に実行します。
- **深い再帰の安全性:** 管理されたフレームスタックにより、再帰アルゴリズムにおけるOSスタックオーバーフローを排除します。
- **透過的なEffect委譲:** Effectを伴う式 (`llm`, `tool`, `computer`, `workspace`) は、`DelegateCall` および `DelegateExpr` を介してVMからランタイムエバリュエータへシームレスに委譲されます。
- **デフォルトエンジン:** `nudgec run` およびスタンドアロンバイナリでは、デフォルトでVMが動作します。`NUDGE_VM=0` を設定することでASTインタープリタへフォールバックできます。

### スタンドアロンネイティブエージェント（Cargo不要）

ビルド環境のツールチェーンに依存しない、単一の自己完結型ネイティブ実行可能バイナリにエージェントをパッケージ化します：

```sh
# エージェントをスタンドアロンバイナリにパッケージ化
nudgec build --standalone my-agent.ndg -o bin/my-agent

# ホスト上で直接実行（Python/Node/Rustへの依存はゼロ）
./bin/my-agent
```

- **ASTスタンピング:** `nudgec` はソースコードを型検査し、ASTバンドルをbincodeへシリアル化して、ビルド済みの `nudge-agent` ランタイムバイナリ（`NDGAGNT1` フッター）へミリ秒単位で追加します。
- **2種類のランタイムフレーバー:**
  - **Full（約2.09MB、デフォルト）:** ライブLLMおよびMCPサーバー向けに `ureq` + `rustls` HTTP/TLS スタックを組み込んだライブ実行対応ランタイム。
  - **Replay-Only（約832KB）:** ネットワーク依存関係を排除し、記録されたNTFトレースからすべての操作を提供する、$0オフライン実行用の軽量ランタイム。
- **ランタイム検出:** `NUDGE_AGENT_RUNTIME` 環境変数で指定されたパス、または `nudgec` と同じディレクトリにあるバイナリから検出されます。[スタンドアロンドキュメント](docs/standalone.md) をご覧ください。

### WebAssembly ターゲット

ブラウザサンドボックス、エッジランタイム（Cloudflare Workers、Fastly）、およびWASI環境へエージェントを展開できます：

- **オフラインリプレイ (`nudge-replay-wasi`):** 記録されたNTFトレースを、`wasmtime` 下の約526KBの `.wasm` モジュール内でオフラインリプレイし、ネイティブ実行とバイト単位で同一の出力を得られます：
  ```sh
  wasmtime run --dir . nudge-replay-wasi.wasm bundle.json trace.jsonl
  ```
- **ライブツールホストブリッジ:** ブラウザやエッジJavaScriptランタイム内のエージェントは、標準化された `nudge_host_tool_call` インポート規約を通じてライブツール呼び出しを行えます（Node WASI下で検証済み、`examples/wasm-host/` 参照）。
- **サンドボックスセキュリティ:** `wasm32` ではネットワークソケットおよびOSプロセス生成が除外され、安全なサンドボックス実行が保証されます。[WebAssembly ドキュメント](docs/wasm.md) をご覧ください。

## ツールと統合

### MCP 統合

stdio 経由で通信する Model Context Protocol (MCP) サーバーを利用したツールを宣言します：

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

`NUDGE_MCP_SERVERS` を通じてライブサーバーを設定します：

```sh
export NUDGE_MCP_SERVERS='{"search": {"command": "mcp-server-websearch"}}'
nudgec run search_agent.ndg
```

パラメータは FastMCP や標準サーバーとの互換性のため、名前付き JSON 引数オブジェクトとして渡されます。サーバーが見つからない場合は、わかりやすい診断情報ととも即座に失敗します。

### 決定論的トレースとエージェントCI

すべての実行は、すべてのEffect操作を記録したオープンな NTF v1 JSONL トレースを出力します：

```sh
# オープン仕様に準拠しているかトレースの整合性を検証
nudgec trace-check trace.jsonl

# 2つのトレースを比較し、コストや動作の変化を特定（--json で機械可読レポートを出力）
nudgec trace-diff baseline.jsonl candidate.jsonl --fail-on-regression --json
```

**GitHub Action (`action.yml`):** APIキー不要、$0コストでプッシュごとにCI上でエージェントをテスト：

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

凍結されたNTF仕様は [docs/ntf-spec.md](docs/ntf-spec.md) にあり、[`conformance/`](conformance/) 内の完全なテストスイート、および LangChain、CrewAI、LangGraph 用のブリッジが提供されています。

### 開発者ツール

- **静的コスト見積もり:** `nudgec cost <file.ndg> [--json]` コンパイル時に各関数の最悪ケースのトークンコストと予算境界を算出します。
- **プロパティおよびファズテスト:** `nudgec fuzz [file.ndg] [--iterations N] [--seed S] [--json]` 変異入力を用いたファズテストと不変プロパティ検証を実行します。
- **データセット評価:** `nudgec eval <file.ndg> --dataset <rows.jsonl> [--fn <name>] [--path <dotted>] [--min-accuracy 0.8] [--json]` JSONL データセットに対してエージェント関数を評価し、精度しきい値で CI をゲートします。
- **評価結果の比較:** `nudgec compare <results_a.jsonl> <results_b.jsonl> [--json]` 2つの評価実行結果を比較し、精度や性能のリグレッションを検出します。
- **実行サマリー（Run Explainer）:** `nudgec explain <trace.jsonl> [--json]` 失敗、トークン消費量、低確信度の判断を強調した人間が読める CLI サマリーまたは JSON 分析を出力します。
- **トレース差分比較:** `nudgec trace-diff <a.jsonl> <b.jsonl> [--json] [--fail-on-regression]` 2つの実行トレースを比較してリグレッションを検出し、ゲート時は終了コード 1 を返します。
- **コンプライアンス監査:** `nudgec audit <trace.jsonl> [--json]` NTF トレースを欧州 AI 法（EU AI Act）の要件（透明性、ガバナンス、未墨消しシークレット、リスク指標）に照らして監査します。
- **ローカルトレースビューア:** `nudgec trace-view <trace.jsonl> [--port N] [--no-open]` 実行タイムライン、トークン消費、修復ハイライトを表示するローカル Web UI を起動します。
- **フォーマッタおよびウォッチャー:** `nudgec fmt <file.ndg> [--check] [--json]` 安全にコードを整形し、`nudgec check <file.ndg> [--json] --watch` で保存ごとに型検査を実行します。
- **リンターと自動修正:** `nudgec lint <file.ndg> [--fix] [--json]` ベストプラクティス（上限なし予算、曖昧なプロンプト、修復なしスキーマ）を検査し、安全な自動修正を適用します。
- **実行ストアインスペクタ:** `nudgec runs [--run <run_id>] [--json]` 耐障害性エージェント実行によって記録された永続実行状態、トレース、チェックポイントを確認します。
- **認証マネージャー:** `nudgec auth login [provider]` および `nudgec auth status [--json]` OAuth 認証情報を管理し、接続済み AI プロバイダーを確認します。
- **静的 HTML エクスポート:** `nudgec trace-html <trace.jsonl> [-o report.html]` 単一ファイル形式の対話型監査レポートを出力します。

### VS Code 拡張機能

シンタックスハイライト、コードスニペット、ホバードキュメント、そして `nudgec lsp` によるリアルタイムの型診断を利用するには、[Nudge Language Extension](https://marketplace.visualstudio.com/items?itemName=Nekomya.nudge-lang) をインストールしてください。

## プライバシーとトレース墨消し（リダクション）

AIエージェントは機密性の高いトークン、認可ヘッダー、秘密鍵、ユーザーの個人情報（PII）を頻繁に取り扱います。Nudge は Fail-Closed なトレース墨消しパイプラインでデータを保護します：

- **単一のチョークポイント:** 墨消しはレコードがディスクやメモリバッファへシリアル化される前の `TraceWriter::emit` 内部で発生します。すべてのレコードタイプ (`llm.call`, `tool.call`, `decision.call`, `computer.*`, `fn.return`) がこの単一のボトルネックでマスクされます。
- **デフォルトで Fail-Closed:** トレースの墨消しはデフォルトで有効です (`NUDGE_TRACE_REDACT=1`)。明示的なローカルデバッグ時のみ `NUDGE_TRACE_REDACT=0` を設定してください。
- **`NUDGE_TRACE_REDACT=0` は利便性のための設定ではなくセキュリティ境界です:** 墨消しを無効にすると、シークレット、APIキー、認可ヘッダー、個人データがトレースファイル内に**プレーンテキスト**で記録されます。これは無効化スイッチの文書化された意図的な動作（テストで保証）です。CI、共有/コミットされる成果物、またはNudge上で構築されたアプリがトレースを処理する場面では絶対に使用しないでください。下流の利用者は `NUDGE_TRACE_REDACT=0` の存在をセキュリティ上の重要なフラグとして扱い、そのモードでトレースを保存または共有しても安全であると仮定してはなりません。
- **自動シークレット検出:** `_KEY`, `_SECRET`, `_TOKEN`, `_PASSWORD`, `_PAT` で終わる、または `AUTH` を含む環境変数（6文字以上の値を持つもの）を自動的に特定してマスクします。
- **階層的なパターンマスキング:** Bearerトークン、認可ヘッダー、Key-Value認証情報ペア、秘密鍵ブロック、ベンダーのトークンプレフィックス（OpenAI `sk-...`, Anthropic `sk-ant-...`, GitHub `ghp_...`, AWS `AKIA...`, Slack `xox...`）をクレンジングします。
- **暗号学的なリプレイ不変性:** コンテンツアドレス指定された `request_hash` は**墨消し前**の入力に基づいて計算されます。これにより、ディスクに認証情報を永続化することなく、マスクされたトレースがO(1)照合によるリプレイ検索時にもバイト単位で同一であることが保証されます。

## 環境変数

| 変数名 | 対象 | 用途 | デフォルト値 |
|:---|:---|:---|:---|
| `NUDGE_PROVIDER` | both | アクティブなLLMプロバイダー (`fake`, `openai`, `anthropic`, `gemini`, `groq`, `ollama`) | `fake` |
| `NUDGE_VM` | native | Bytecode VM 実行エンジンの切り替え (`1` = VM デフォルト, `0` = AST インタープリタフォールバック) | `1` |
| `NUDGE_DEFAULT_MODEL` | native | `with` ブロックで明示的に指定されていない場合のデフォルトモデル文字列 | プロバイダーデフォルト |
| `NUDGE_API_KEY` / `NUDGE_BASE_URL` | both | OpenAI互換LLMプロバイダー向けの認証情報およびベースエンドポイント | なし |
| `NUDGE_PRICING` | both | カスタムJSON価格設定テーブル: `{"model": [usd_per_1M_in, usd_per_1M_out]}` | 組み込みテーブル |
| `NUDGE_GUARD` | python/TS | 出力ガード（`pii` はLLM出力内のシークレット、メールアドレス、IPアドレスをマスク） | なし |
| `NUDGE_BUDGET` | both | 並行ファンアウト全体で適用される実行全体でのUSD消費上限 | なし |
| `NUDGE_REPAIR_BUDGET` | python | 修復ラウンド用の累積USD上限 | なし |
| `NUDGE_MAX_LOOP_ITERATIONS` | native | 実行時エラー `E0703` を発生させるまでの最大シーケンシャルループ反復回数 | `10000` |
| `NUDGE_TRACE` | both | NTF JSONL 実行トレースの出力先パス | `trace.jsonl` |
| `NUDGE_REPLAY` | both | 決定論的オフラインリプレイ用トレースパス (`NUDGE_REPLAY_MODE=all` または `llm`) | なし |
| `NUDGE_RESUME` | both | 記録されたトレースプレフィックスを使用してチェックポイントからクラッシュした実行を再開 | なし |
| `NUDGE_TRACE_REDACT` | native | トレース内の単一チョークポイントでのシークレットおよびPIIマスキングの有効化 (`1` または `0`) | `1` (有効) |
| `NUDGE_TRACE_REDACT_ENV` | native | トレース内のシークレット環境変数の値を自動的にマスク | `1` (有効) |
| `NUDGE_TRACE_REDACT_KEYS` | native | マスク対象に追加するカンマ区切りのJSONオブジェクトキーリスト | なし |
| `NUDGE_TRACE_REDACT_PATTERNS` | native | カスタムトレースマスキング用のカンマ区切り正規表現リストまたは正規表現ファイルへのパス | なし |
| `NUDGE_WORKSPACE_DIR` | native | サンドボックス化された `workspace.*` 操作のルートディレクトリ権限 | カレントディレクトリ |
| `NUDGE_STORE_PATH` | native | 永続的な型付きKey-Valueデータの保存先ファイルパス | `<workspace>/.nudge/store.json` |
| `NUDGE_UI_HEADLESS` | native | UIウィンドウ操作をメモリ内でヘッドレス実行することを強制 (`1` または `true`) | `0` |
| `NUDGE_TOOL_GRANTS` | both | 実行レイヤーのJSONツールポリシー (例: `{"web_search": ["*"]}`)。Fail-Closed | なし |
| `NUDGE_PROCESS_GRANTS` | native | `proc.*` コマンドおよび引数に対する Fail-Closed なJSON実行ポリシー | なし (拒否) |
| `NUDGE_PROCESS_TIMEOUT` | native | `proc.*` のデフォルト子プロセス実行タイムアウト（ミリ秒） | `30000` |
| `NUDGE_FS_WATCH_TIMEOUT` | native | `fs.watch` のデフォルトポーリングおよび待機タイムアウト（ミリ秒） | `30000` |
| `NUDGE_ENABLE_ANTIGRAVITY` | native | Antigravity プロバイダーを有効化するオプトイン切り替え (`1` または `true`) | `0` |
| `NUDGE_AUTH_ANTIGRAVITY` | native | Antigravity プロバイダー認証用の直接Bearerトークン | なし |
| `NUDGE_AUTH_GITHUB_COPILOT` | native | GitHub Copilot プロバイダー認証用の直接トークンオーバーライド | なし |
| `NUDGE_MCP_SERVERS` | both | MCPサーバー名をstdioコマンドにマッピングするJSONレジストリ | なし |
| `NUDGE_MCP_TIMEOUT_MS` | native | MCPサーバーのリクエストごとの読み取り期限およびセッションハングタイムアウト | `30000` |
| `NUDGE_DECISION_SERVERS` | both | 判定モデルエンドポイントのJSONレジストリ (例: `{"laya": {"base_url": "..."}}`) | なし |
| `NUDGE_DECISION_CACHE` | both | 永続化された判定モデル分布のキャッシュファイルパス | なし |
| `NUDGE_COMPUTER_SERVERS` | both | コンピュータ操作ブリッジ用のJSONレジストリ（サブプロセスJSONLまたはHTTP） | なし |
| `NUDGE_COMPUTER_PROVIDER` | both | コンピュータ操作に選択されたブリッジ (`fake`、または登録されたサーバー名) | `fake` |
| `NUDGE_COMPUTER_KILL` | both | ハードキルスイッチ: `1` の場合はすべてのコンピュータ操作を即座に拒否 | `0` |
| `NUDGE_COMPUTER_DRIFT` | both | `1` の場合、リプレイ中にライブ状態を再観測し、機械的なドリフト差分を付与 | `0` |
| `NUDGE_AGENT_RUNTIME` | native | `nudgec build --standalone` 用のビルド済み `nudge-agent` バイナリのパス | 同一階層のバイナリ |
| `NUDGE_CREDENTIALS_PATH` | native | Nudge認証情報ストアのカスタムパス | `~/.nudge/credentials.json` |
| `NUDGE_OTEL` | python | OpenTelemetry スパンのエクスポート先ファイル | なし |
| `NUDGE_OTEL_ENDPOINT` | python | OpenTelemetry HTTP エクスポートエンドポイント | なし |

`native` はRustランタイム (`nudgec`/`nudge-runtime`)、`python/TS` は従来の生成ランタイム、`both` は両方のランタイムファミリーに適用されます。

## 実践的なサンプル

[`examples/`](examples/) で完全なエージェント実装を確認できます：

| サンプル | ドメイン | ハイライト |
|:---|:---|:---|
| [AI Chatbot](examples/chatbot/) | 対話型アシスタント | 型付きメモリスキーマ、決定論的リプレイ、予算上限 |
| [Triage Agent](examples/triage-agent/) | サポート分類 | バッチ化された `decide{}` ブロック、信頼度しきい値、ルーティングポリシー |
| [Code Reviewer](examples/code-reviewer/) | ソフトウェアエンジニアリング | 構造化構文診断、自動化されたルーブリックスコアリング |
| [Research Agent](examples/research_agent.ndg) | 複数ソースの統合 | `par map` による並列取得、信頼度スコアリング |
| [Computer Automation](examples/computer-automation/) | デスクトップエージェント | 画面観測、アクセシビリティツリー、ドリフト検証 |
| [RAG Agent](examples/rag-agent/) | 検索拡張生成 | MCP stdio 検索ツールバインディング、引用抽出 |
| [Property Fuzzer](examples/property-fuzz/) | エージェントテスト | `for_all` 不変条件テスト、プロンプトインジェクションファジング |
| [Dungeon Crawler](examples/dungeon-crawler/) | 状態マシンシミュレーション | 純粋Nudge実装、エージェント状態遷移、ターン制戦闘 |

## ライセンスとソースからのビルド

Nudge は **Apache-2.0** ライセンスのもとで公開されているオープンソースです。詳細は [LICENSE](LICENSE) をご覧ください。

### ソースからのビルド

コンパイラのビルドには Rust 1.75+（安定版）が必要です：

```sh
git clone https://github.com/NekomyaDev/nudge.git
cd nudge
cargo build --release -p nudgec   # compiler binary at target/release/nudgec
cargo test --workspace            # compiler, VM, and runtime test suite
```

Python ランタイムは `runtime/` に配置されています（標準ライブラリのみ、外部依存ゼロ）：

```sh
pip install ./runtime
```

---

<p align="center">
  Made with love by <a href="https://github.com/NekomyaDev">NekomyaDev</a>
</p>
