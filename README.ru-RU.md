<p align="center">
  <img src="assets/logo.svg" width="200" height="200" alt="Nudge Logo">
</p>

<h1 align="center">Nudge</h1>

<p align="center">
  <strong>Не парсите ваших агентов. Направляйте их с Nudge.</strong><br>
  Типизированный, воспроизводимый (replayable) язык программирования с контролем budget для AI-агентов.<br>
  Нативная среда выполнения bytecode и автономный компилятор. Компилируется в Python и TypeScript.
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
  <a href="README.tr-TR.md">Türkçe</a>
</p>

---

## Почему Nudge?

Продакшен-агенты до сих пор держатся на связующем коде (glue code): цепочки prompt, разбираемые вручную, вызовы tool, обернутые в блоки try/except, отсутствие replayability, непредсказуемые расходы и полное отсутствие регрессионных тестов. Библиотеки лишь маскируют симптомы после их появления. **Nudge устраняет проблему на том уровне, где она возникает: на уровне языка программирования.**

| Проблема | Библиотеки | Nudge |
|:---|:---|:---|
| Нетипизированный вывод LLM | Валидация во время выполнения | Schema является первоклассным типом языка, проверяемым во время компиляции |
| Скрытые побочные эффекты | Невидимы | 10 явных типизированных effect, объявленных в сигнатурах (`uses LLM, Tool, IO, ...`) |
| Нет регрессионного тестирования | Запись и replay прикручены поверх | Каждый запуск создает NTF trace, и каждый trace является детерминированным тестом за $0 |
| Неожиданные расходы | Отслеживание постфактум после счета | Budget является строгим контрактом, соблюдение которого гарантируют компилятор и runtime |
| Спагетти из асинхронного кода | Ручные циклы async | Нативные конструкции `par map / race / all` с проверкой race safety во время компиляции |

## Знакомство с Nudge

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

Компилятор проверяет соответствие schema, выводит используемые effect и ограничивает максимальную стоимость выполнения. Среда выполнения записывает каждый вызов в контентно-адресуемый trace, который можно сравнивать через diff, коммитить в репозиторий и воспроизводить (replay) за $0.

## Быстрый старт

### 1. Установка Nudge

**Установка одной командой (рекомендуется):**

```sh
# Linux / macOS
curl -fsSL https://raw.githubusercontent.com/NekomyaDev/nudge/main/install.sh | bash

# Windows (PowerShell as Administrator)
irm https://raw.githubusercontent.com/NekomyaDev/nudge/main/install.ps1 | iex
```

**Пакетные менеджеры и контейнеры:**

```sh
# Snap (Linux)
sudo snap install nudge --classic

# Docker
docker run -it --rm -v $(pwd):/workspace nekomyadev/nudge nudgec --help
```

Скомпилированные бинарные файлы для Linux, macOS и Windows также доступны на странице [Releases](https://github.com/NekomyaDev/nudge/releases).

### 2. Первый запуск за 60 секунд (Native, $0)

Никакого Python, Node.js или API-ключей не требуется. Nudge включает нативную стековую виртуальную машину и детерминированный in-memory fake provider:

```sh
# Create a program
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

# Type-check your program
nudgec check hello.ndg

# Run directly on the native Bytecode VM ($0, no keys needed)
nudgec run hello.ndg

# Run offline regression tests
nudgec test hello.ndg
```

### 3. Интерактивные уроки и шаблоны проектов

```sh
# Six interactive lessons in your terminal (hello, types, decide, policy, test, trace)
nudgec learn

# Scaffold a project from ready-made templates (triage, chatbot, rag-agent, etc.)
nudgec init my-agent --template triage
cd my-agent && nudgec check my-agent.ndg && nudgec run my-agent.ndg
```

### 4. Компиляция в Python или TypeScript (опционально)

Если вы развертываете агента внутри существующего сервиса на Python или TypeScript:

```sh
# Python target
pip install nudge-runtime          # standard library only, zero dependencies
nudgec build hello.ndg              # emits out/hello.py
python3 out/hello.py

# TypeScript target
nudgec build-ts hello.ndg             # emits out/hello.ts
```

## Краткий обзор основных возможностей

| Возможность | Что обеспечивает | Раздел |
|:---|:---|:---|
| **Типизированные вызовы LLM** | Схема вывода является типом, а нарушения запускают автоматическое исправление | [Знакомство с Nudge](#знакомство-с-nudge) |
| **Управление потоком и данные** | `if/else`, циклы `for`, литералы записей `{k: v}`, индексация, мутабельные переменные | [Управление потоком и данные](#управление-потоком-и-данные) |
| **Типизированные Effect** | 10 известных effect, отслеживаемых и проверяемых во время компиляции | [Типизированные Effect](#типизированные-effect) |
| **Workspace и Store** | Изолированная файловая система в песочнице + типизированное постоянное хранилище key-value | [Изолированный Workspace и постоянный Store](#изолированный-workspace-и-постоянный-store) |
| **Локальный UI** | Типизированные view, окна, headless-тесты и нативное десктопное окно ОС | [Локальный UI](#локальный-ui) |
| **Задачи и параллелизм** | Запуск и ожидание задач (spawn/join), кооперативная отмена, безопасный `par race`, освобождение budget | [Жизненный цикл задач и параллелизм](#жизненный-цикл-задач-и-параллелизм) |
| **Аутентификация провайдеров** | Flow авторизации устройств RFC 8628 OAuth по принципу fail-closed, без сбора сторонних токенов | [Аутентификация провайдеров](#аутентификация-провайдеров) |
| **Типизированные решения** | Пакетная классификация, да/нет, оценка по рубрикам над состояниями (`/v1/systemone`) | [Типизированные решения](#типизированные-решения) |
| **Computer Use** | Наблюдение за экраном, типизированные действия, список разрешенных приложений, kill switch | [Управление компьютером (Computer Use)](#управление-компьютером-computer-use) |
| **Bytecode VM** | Стек-машина по умолчанию (~70x быстрее чистых вычислений, безопасность глубокой рекурсии) | [Движок Bytecode VM](#движок-bytecode-vm) |
| **Автономные агенты** | Упаковка в единый бинарный файл без участия Cargo (~2.09MB live, ~832KB replay) | [Автономные нативные агенты (без Cargo)](#автономные-нативные-агенты-без-cargo) |
| **WebAssembly** | Офлайн-replay в WASI (~526KB), мост к хосту живых инструментов | [Таргет WebAssembly](#таргет-webassembly) |
| **Детерминированные Trace** | Открытые NTF v1 trace, replay за $0, регрессионные проверки в CI, сравнение trace (diff) | [Детерминированные Trace и CI для агентов](#детерминированные-trace-и-ci-для-агентов) |
| **Маскирование приватных данных** | Автоматическое маскирование секретов и PII по умолчанию в единой точке контроля | [Конфиденциальность и маскирование Trace](#конфиденциальность-и-маскирование-trace) |

## Язык и возможности

### Управление потоком и данные

Nudge предоставляет привычные конструкции структурного программирования с детерминированными границами безопасности:

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

- **Ветвление:** выражения `if/else` и `else if`, поддерживающие блоки ветвления из нескольких операторов.
- **Итерация:** последовательные циклы `for item in coll` и индексированные циклы `for (item, idx) in coll`.
- **Структурированные данные:** литералы записей `{ field1: expr1, field2: expr2 }` со статической типизацией и доступом к полям (`obj.field`).
- **Индексация:** индексированный доступ к коллекциям и записям (`items[0]`, `map["key"]`). Выход за границы диапазона корректно завершается ошибкой `E0705`.
- **Мутабельные локальные переменные:** явные переменные `let mut name = init` с поддержкой переприсваивания (`name = next`).
- **Ограничения циклов:** число итераций ограничено параметром `NUDGE_MAX_LOOP_ITERATIONS` (по умолчанию 10 000) для предотвращения бесконечного выполнения, с вызовом ошибки `E0703` при превышении лимита.

### Типизированные Effect

Каждый побочный эффект декларируется в сигнатуре функции и проверяется компилятором:

```nudge
fn run_pipeline() -> bool uses LLM, Tool, Workspace, Store {
    // Calling unlisted effects triggers compiler error E0301
}
```

Nudge определяет 10 известных effect (`crates/nudgec/src/check.rs`):
- `LLM`: Вызывает генерацию ответа языковой моделью или потоковые prompt.
- `Tool`: Вызывает внешние MCP или зарегистрированные инструменты.
- `IO`: Операции ввода/вывода хоста (`print`, `println`).
- `Decision`: Запрашивает модели принятия решений через `decide{}`.
- `Computer`: Взаимодействует с окружением рабочего стола и деревьями доступности через `computer.*`.
- `Workspace`: Изолированные операции чтения и записи внутри защищенного каталога workspace.
- `WorkspaceDelete`: Деструктивное удаление файлов или каталогов внутри workspace.
- `Store`: Операции с постоянным хранилищем ключ-значение через `store.*`.
- `UI`: Создает, монтирует или отправляет события графическим элементам интерфейса.
- `Auth`: Обращается к аутентифицированным ресурсам провайдеров (поддерживает параметризованный `Auth(account)`).

Функции без секции `uses` считаются чистыми (pure), что доказывается во время компиляции. Неизвестные effect отклоняются с ошибкой `E0302`.

### Изолированный Workspace и постоянный Store

Локальные агенты должны работать с файлами и сохранять состояние между запусками без ущерба для безопасности системы:

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

- **Изолированный Workspace (`workspace.*`):** все операции ограничены явным корневым каталогом workspace (настраивается через `--workspace` или `NUDGE_WORKSPACE_DIR`, по умолчанию текущий каталог). Попытки обхода путей (`../`) и выход за пределы корня через симлинки блокируются. Запись устойчива к сбоям благодаря атомарной схеме (запись во временный файл и последующее переименование).
- **Разделение полномочий на удаление:** для удаления файлов или каталогов требуется специальный effect `WorkspaceDelete` (`uses WorkspaceDelete`), что защищает каталоги от случайного удаления обычными функциями записи.
- **Постоянное хранилище ключ-значение (`store.*`):** строго типизированное хранилище, сохраняемое в `<workspace>/.nudge/store.json` (или `NUDGE_STORE_PATH`). В отличие от временных чекпоинтов выполнения, эти данные сохраняются между отдельными запусками и перезагрузками системы.

### Отслеживание процессов и файлов

Создавайте агентов, управляющих раннерами сборки и тестов, языковыми серверами и инструментами live-reload, с той же дисциплиной effect и прав доступа grants:

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

- **Отслеживание файлов (`fs.watch`, effect `FsWatch`):** мониторинг пути внутри workspace на наличие событий изменений, ограниченный таймаутом (никогда не зависает бесконечно) и замкнутый в корневом каталоге workspace (выход за границы блокируется). Недекларированное использование пресекается типизацией (`E0301`).
- **Контролируемые процессы (`proc.*`, effect `Process`):** функции `proc.spawn(cmd, argv, timeout_ms)`, `proc.run(...)` (spawn + await), `proc.await`, `proc.kill`, `proc.alive`. Исполняемый файл `cmd` и его аргументы `argv` передаются **раздельно** и выполняются напрямую без оболочки shell (исключается уязвимость к shell-инъекциям). Жизненный цикл ограничен по времени (`E0921`), появление **процессов-сирот исключено** (дочерние процессы принудительно завершаются при выходе или отмене задачи), поддерживается kill-on-cancel. Выполнение любых команд работает по принципу **fail-closed**: запрещено по умолчанию, если оно явно не разрешено в `NUDGE_PROCESS_GRANTS` (`E0920`, изоляция workspace `E0922`).

### Локальный UI

Создавайте нативные интерфейсы с участием человека (human-in-the-loop), станции подтверждения действий (approval stations) и интерактивные панели инспекции прямо в Nudge:

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

- **Два режима выполнения:**
  - **Headless-тестирование:** `mount_headless()` и `dispatch_click()` вычисляют деревья UI и смену состояний полностью в оперативной памяти без открытия физического окна. Работает в CI за $0.
  - **Нативный десктопный GUI:** при компиляции с Cargo-фичей `native-gui` вызов `window.open()` открывает нативное окно операционной системы на базе библиотек `winit` и `softbuffer`.
- **Автоматический откат в Headless:** установка переменной `NUDGE_UI_HEADLESS=1` или запуск в режиме воспроизведения trace (`NUDGE_REPLAY`) автоматически перенаправляет вызовы окон в headless-движок.

### Жизненный цикл задач и параллелизм

Координируйте параллельные потоки агента с формальным управлением состоянием задач и кооперативной отменой:

```nudge
fn race_models(prompt: string) -> string uses LLM {
    // par race automatically cancels losing branches
    par race {
        fast: query_fast_model(prompt),
        deep: query_deep_model(prompt)
    }
}
```

- **Кооперативная отмена:** когда результат гонки определен, проигравшие ветви останавливаются на границах циклов, потоков данных (stream) и вызовов effect вместо того, чтобы продолжать расходовать budget в фоновом режиме.
- **Защита budget:** неизрасходованные резервации budget в рамках `NUDGE_BUDGET` немедленно освобождаются при отмене задачи.
- **Права fail-closed:** отмененные задачи мгновенно теряют полномочия на отправку последующих вызовов инструментов (`Tool`) или действий с компьютером (`Computer`).
- **Примитивы для работы с задачами:** явное управление задачами через `task.spawn`, `task.join`, `task.status` и `task.cancel`.

### Аутентификация провайдеров

Безопасное управление учетными данными, интегрированное в язык и среду выполнения:

- **Аутентификация fail-closed:** функции объявляют явные требования к авторизации через `uses Auth` или `uses Auth(account)`. Неавторизованные вызовы завершаются с ошибкой (fail-closed).
- **Запрет сбора сторонних токенов:** Nudge никогда не считывает приватные кэши токенов или хранилища учетных данных, принадлежащие сторонним приложениям.
- **Нативный flow устройств RFC 8628:** интерактивная авторизация в терминале через OAuth 2.0 Device Authorization Grant с безопасным хранением учетных данных в `~/.nudge/credentials.json` (права доступа к файлу 0600).
- **Шлюзы провайдеров:** провайдеры обязаны удовлетворять верифицируемым контрактам безопасности (`E-auth-0401` при отсутствии учетных данных, `E-auth-0403` при нехватке прав scope, `E-auth-0404` для отключенных провайдеров).
- **Неподтвержденные провайдеры остаются закрытыми (Permission Gate):** провайдер, для которого официальное разрешение на использование сторонними клиентами не доказано, **по умолчанию отключен (fail-closed)**. В настоящее время это относится к `antigravity`: логика авторизации OAuth и код провайдера реализованы, однако официальное подтверждение того, что его клиент OAuth и эндпоинты моделей разрешено вызывать из независимого стороннего клиента, отсутствует. Поэтому шлюз остается **отключенным** (`E-auth-0404`) до подтверждения разрешения. **Тест с мок-сервером OAuth или зеленый запуск в CI НЕ доказывают доступ к реальному аккаунту или наличие разрешения на использование.** Пользователи (включая любые приложения на базе Nudge) не должны полагаться на это подключение до документирования разрешений. Явный флаг пользователя (`NUDGE_ENABLE_ANTIGRAVITY=1`) активирует его на собственный страх и риск после подтверждения прав.

### Типизированные решения

Компилируйте неопределенность от моделей принятия решений (таких как Laya и Jev) через сетевой протокол `/v1/systemone`:

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

- **Пакетное выполнение:** один вызов оценивает все вопросы одновременно, возвращая победителя, вероятность, распределение и показатель уверенности (confidence).
- **Тестирование за $0:** стандартный fake decision provider синтезирует детерминированные распределения на основе seed для тестовых прогонов за $0.
- **Живые провайдеры:** подключение локальных или удаленных моделей через `NUDGE_DECISION_SERVERS='{"laya": {"base_url": "http://localhost:8000"}}'`.
- **Кэш решений:** параметр `NUDGE_DECISION_CACHE=<path>` кэширует проверенные ответы между запусками, исключая повторную оплату за одинаковое состояние.
- **Анализ политик (Policy Sweeps):** запуск `nudgec policy-sweep trace.jsonl --question dept --thresholds 0.5,0.8` позволяет оценить калибровку порогов по записанным распределениям без единого вызова модели. Читайте [документацию по решениям](docs/decision.md).

### Управление компьютером (Computer Use)

Управляйте десктопным окружением через типизированные наблюдения и действия, где язык обеспечивает контроль и безопасность, а модель отвечает за восприятие:

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

- **Типизированные примитивы:** `computer.observe` возвращает запись `Observation` (дерево доступности, элементы, опциональный скриншот). Действия (`click`, `type`, `key`, `scroll`, `set_value`, `drag`, `paste`) возвращают `ActionResult` с квитанцией об отправке (`action_sent`).
- **Безопасность replay:** повторное воспроизведение trace никогда не выполняет физические действия повторно. С флагом `NUDGE_COMPUTER_DRIFT=1` среда выполнения заново считывает живое состояние и рассчитывает механическую разницу `Drift` без совершения действий.
- **Усиление защиты:** список разрешенных приложений (`allow: [app]`), блокировки аренды контроллера, отклонение устаревших индексов элементов (`stale_state`) и жесткий выключатель аварийной остановки (`NUDGE_COMPUTER_KILL=1`).
- **Детерминированный десктоп:** стандартный fake provider выполняет сценарии UI в памяти (`NUDGE_COMPUTER_SCENARIO`), обеспечивая проведение сквозных тестов за $0. Читайте [документацию по Computer Use](docs/computer-use.md).

## Нативная инфраструктура

### Движок Bytecode VM

Чистые функции, трансформации данных и ветвление маршрутов исполняются на специализированной стековой виртуальной машине bytecode (`crates/nudgec/src/bytecode.rs` и `vm.rs`):

- **Высокоскоростные чистые вычисления:** выполняет чистый код примерно в 70 раз быстрее интерпретатора AST.
- **Безопасность глубокой рекурсии:** управляемый стек фреймов предотвращает переполнение стека операционной системы в рекурсивных алгоритмах.
- **Прозрачное делегирование effect:** выражения с побочными эффектами (`llm`, `tool`, `computer`, `workspace`) прозрачно передаются из виртуальной машины исполнителю среды выполнения через `DelegateCall` и `DelegateExpr`.
- **Движок по умолчанию:** виртуальная машина активна по умолчанию для `nudgec run` и автономных бинарных файлов. Установите `NUDGE_VM=0` для возврата к интерпретатору AST.

### Автономные нативные агенты (без Cargo)

Упаковывайте агента в единый самодостаточный нативный исполняемый файл без необходимости наличия компилятора или сборочных инструментов на целевой машине:

```sh
# Package an agent into a standalone binary
nudgec build --standalone my-agent.ndg -o bin/my-agent

# Run directly on the host (zero Python/Node/Rust dependencies)
./bin/my-agent
```

- **Штамповка AST:** `nudgec` проверяет типы, сериализует bundle AST в bincode и присоединяет его к готовому исполняемому файлу runtime `nudge-agent` (с маркером `NDGAGNT1` в конце файла) за доли секунды.
- **Две разновидности runtime:**
  - **Полный (~2.09MB, по умолчанию):** среда с поддержкой сетевого взаимодействия через встроенный стек HTTP/TLS на базе `ureq` + `rustls` для работы с живыми LLM и MCP-серверами.
  - **Только Replay (~832KB):** компактная среда без сетевых библиотек, обслуживающая все вызовы из предварительно записанных NTF trace для работы в офлайне за $0.
- **Поиск runtime:** путь определяется через переменную `NUDGE_AGENT_RUNTIME` или по соседству с бинарным файлом `nudgec`. Читайте [документацию по автономным агентам](docs/standalone.md).

### Таргет WebAssembly

Развертывайте агентов в изолированных песочницах браузеров, средах edge (Cloudflare Workers, Fastly) и окружениях WASI:

- **Офлайн-replay (`nudge-replay-wasi`):** воспроизведение записанных NTF trace внутри модуля `.wasm` размером около 526KB под управлением `wasmtime` с побайтово идентичным выводом по сравнению с нативным запуском:
  ```sh
  wasmtime run --dir . nudge-replay-wasi.wasm bundle.json trace.jsonl
  ```
- **Мост к хосту живых инструментов:** агенты в браузере или среде JavaScript edge совершают вызовы внешних инструментов через стандартизированный контракт импорта `nudge_host_tool_call` (протестировано в Node WASI, см. `examples/wasm-host/`).
- **Безопасная песочница:** сетевые сокеты и порождение процессов ОС заблокированы на уровне платформы `wasm32`, что гарантирует изолированное выполнение. Читайте [документацию по WebAssembly](docs/wasm.md).

## Инструменты и интеграции

### Интеграция с MCP

Объявляйте инструменты, работающие на базе серверов Model Context Protocol (MCP), обменивающихся данными через stdio:

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

Настройка активных серверов через переменную `NUDGE_MCP_SERVERS`:

```sh
export NUDGE_MCP_SERVERS='{"search": {"command": "mcp-server-websearch"}}'
nudgec run search_agent.ndg
```

Параметры передаются как именованные JSON-объекты аргументов для полной совместимости с FastMCP и стандартными серверами. При отсутствии сервера выполнение быстро прерывается с информативной диагностикой.

### Детерминированные Trace и CI для агентов

Каждый запуск создает открытый trace в формате NTF v1 JSONL, фиксирующий взаимодействия со всеми effect:

```sh
# Verify trace integrity against the open specification
nudgec trace-check trace.jsonl

# Compare two traces to identify cost or behavior changes
nudgec trace-diff baseline.jsonl candidate.jsonl --fail-on-regression
```

**GitHub Action (`action.yml`):** тестирование агентов в CI на каждом push с затратами $0 и без необходимости в API-ключах:

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

Зафиксированная спецификация NTF доступна в файле [docs/ntf-spec.md](docs/ntf-spec.md), а полный набор тестов представлен в каталоге [`conformance/`](conformance/) с готовыми адаптерами для LangChain, CrewAI и LangGraph.

### Инструменты разработчика

- **Локальный просмотрщик Trace:** команда `nudgec trace-view <trace.jsonl>` запускает локальный веб-интерфейс с наглядной шкалой времени выполнения, расходом токенов и отображением раундов восстановления (repair).
- **Анализатор запуска (Run Explainer):** `nudgec explain <trace.jsonl>` выводит сводку в терминал с информацией об ошибках, потреблении токенов и решениях с низкой уверенностью (confidence).
- **Форматирование и отслеживание:** утилита `nudgec fmt <file.ndg>` аккуратно форматирует код, а `nudgec check <file.ndg> --watch` проверяет типы при каждом сохранении файла.
- **Статический экспорт в HTML:** команда `nudgec trace-html trace.jsonl -o report.html` экспортирует автономные интерактивные отчеты для аудита.

### Расширение для VS Code

Установите [расширение языка Nudge](https://marketplace.visualstudio.com/items?itemName=Nekomya.nudge-lang) для подсветки синтаксиса, фрагментов кода, документации при наведении курсора и диагностики типов в реальном времени на базе `nudgec lsp`.

## Конфиденциальность и маскирование Trace

AI-агенты регулярно работают с чувствительными токенами, заголовками авторизации, приватными ключами и персональными данными (PII). Nudge защищает ваши данные с помощью конвейера маскирования trace по принципу fail-closed:

- **Единая точка контроля (Single Choke Point):** маскирование выполняется внутри `TraceWriter::emit` перед сохранением записей на диск или в буферы памяти. Каждый тип записей (`llm.call`, `tool.call`, `decision.call`, `computer.*`, `fn.return`) маскируется в этой единой точке.
- **Fail-Closed по умолчанию:** маскирование trace включено по умолчанию (`NUDGE_TRACE_REDACT=1`). Устанавливайте `NUDGE_TRACE_REDACT=0` исключительно для явной локальной отладки.
- **`NUDGE_TRACE_REDACT=0` является границей безопасности, а не просто опцией:** отключение маскирования приведет к записи секретов, API-ключей, заголовков авторизации и личных данных в **ОТКРЫТОМ ТЕКСТЕ** в файл trace. Это задокументированное и проверенное тестами свойство переключателя. Никогда не используйте его в CI, с коммитируемыми артефактами или в приложениях, потребляющих trace. Все внешние системы должны расценивать флаг `NUDGE_TRACE_REDACT=0` как фактор риска безопасности и не считать такие файлы trace безопасными для хранения или передачи.
- **Автоматическое обнаружение секретов:** система автоматически находит и скрывает значения переменных окружения, имена которых оканчиваются на `_KEY`, `_SECRET`, `_TOKEN`, `_PASSWORD`, `_PAT` или содержат подстроку `AUTH` (при длине значения от 6 символов).
- **Многоуровневая фильтрация паттернов:** удаляет bearer-токены, заголовки авторизации, пары ключ-значение учетных данных, блоки закрытых ключей и префиксы ключей провайдеров (OpenAI `sk-...`, Anthropic `sk-ant-...`, GitHub `ghp_...`, AWS `AKIA...`, Slack `xox...`).
- **Криптографическая инвариантность replay:** контентно-адресуемый хеш `request_hash` рассчитывается на исходных данных **до маскирования**. Благодаря этому скрытые trace сохраняют побайтовое соответствие при поиске во время replay со сложностью O(1), не сохраняя секреты на диске.

## Переменные окружения

| Переменная | Область | Назначение | По умолчанию |
|:---|:---|:---|:---|
| `NUDGE_PROVIDER` | both | Активный провайдер LLM (`fake`, `openai`, `anthropic`, `gemini`, `groq`, `ollama`) | `fake` |
| `NUDGE_VM` | native | Переключатель движка исполнения Bytecode VM (`1` = VM по умолчанию, `0` = откат к интерпретатору AST) | `1` |
| `NUDGE_DEFAULT_MODEL` | native | Модель по умолчанию, если она явно не указана в блоке `with` | По умолчанию для провайдера |
| `NUDGE_API_KEY` / `NUDGE_BASE_URL` | both | Учетные данные и базовый URL для OpenAI-совместимых провайдеров LLM | Нет |
| `NUDGE_PRICING` | both | Пользовательская таблица стоимости в JSON: `{"model": [usd_per_1M_in, usd_per_1M_out]}` | Встроенная таблица |
| `NUDGE_GUARD` | python/TS | Фильтры вывода (`pii` скрывает секреты, адреса электронной почты, IP-адреса в ответах LLM) | Нет |
| `NUDGE_BUDGET` | both | Лимит затрат в USD на весь запуск, соблюдаемый всеми параллельными потоками | Нет |
| `NUDGE_REPAIR_BUDGET` | python | Совокупный лимит в USD для раундов исправления (repair) | Нет |
| `NUDGE_MAX_LOOP_ITERATIONS` | native | Предельное число последовательных итераций цикла до вызова ошибки времени выполнения `E0703` | `10000` |
| `NUDGE_TRACE` | both | Путь к файлу для записи trace выполнения в формате NTF JSONL | `trace.jsonl` |
| `NUDGE_REPLAY` | both | Путь к trace для детерминированного воспроизведения в офлайне (`NUDGE_REPLAY_MODE=all` или `llm`) | Нет |
| `NUDGE_RESUME` | both | Возобновление прерванного выполнения с чекпоинта по записанному префиксу trace | Нет |
| `NUDGE_TRACE_REDACT` | native | Включение маскирования секретов и PII в trace в единой точке контроля (`1` или `0`) | `1` (Активно) |
| `NUDGE_TRACE_REDACT_ENV` | native | Автоматическое сокрытие значений секретных переменных окружения в trace | `1` (Активно) |
| `NUDGE_TRACE_REDACT_KEYS` | native | Разделенный запятыми список дополнительных ключей JSON-объектов для сокрытия | Нет |
| `NUDGE_TRACE_REDACT_PATTERNS` | native | Регулярные выражения через запятую или путь к файлу regex для пользовательского маскирования | Нет |
| `NUDGE_WORKSPACE_DIR` | native | Корневой каталог для выполнения изолированных операций `workspace.*` | Текущий каталог |
| `NUDGE_STORE_PATH` | native | Путь к файлу хранилища постоянных типизированных данных ключ-значение | `<workspace>/.nudge/store.json` |
| `NUDGE_UI_HEADLESS` | native | Принудительный запуск операций с окнами UI в headless-режиме в памяти (`1` или `true`) | `0` |
| `NUDGE_TOOL_GRANTS` | both | JSON-политика доступа к инструментам на этапе выполнения (например, `{"web_search": ["*"]}`). Принцип fail-closed | Нет |
| `NUDGE_PROCESS_GRANTS` | native | Политика fail-closed в формате JSON для разрешенных команд и аргументов `proc.*` | Нет (запрещено) |
| `NUDGE_PROCESS_TIMEOUT` | native | Таймаут выполнения дочернего процесса по умолчанию в миллисекундах для `proc.*` | `30000` |
| `NUDGE_FS_WATCH_TIMEOUT` | native | Таймаут ожидания изменений по умолчанию в миллисекундах для `fs.watch` | `30000` |
| `NUDGE_ENABLE_ANTIGRAVITY` | native | Флаг явного согласия для включения провайдера Antigravity (`1` или `true`) | `0` |
| `NUDGE_AUTH_ANTIGRAVITY` | native | Прямой bearer-токен для аутентификации в провайдере Antigravity | Нет |
| `NUDGE_AUTH_GITHUB_COPILOT` | native | Прямое переопределение токена для аутентификации в GitHub Copilot | Нет |
| `NUDGE_MCP_SERVERS` | both | JSON-реестр соответствия имен MCP-серверов командам stdio | Нет |
| `NUDGE_MCP_TIMEOUT_MS` | native | Таймаут на чтение запроса и зависшей сессии для серверов MCP | `30000` |
| `NUDGE_DECISION_SERVERS` | both | JSON-реестр эндпоинтов моделей принятия решений (например, `{"laya": {"base_url": "..."}}`) | Нет |
| `NUDGE_DECISION_CACHE` | both | Путь к файлу кэша для сохраненных распределений моделей принятия решений | Нет |
| `NUDGE_COMPUTER_SERVERS` | both | JSON-реестр мостов управления компьютером (подпроцесс JSONL или HTTP) | Нет |
| `NUDGE_COMPUTER_PROVIDER` | both | Выбранный мост для действий с компьютером (`fake` или зарегистрированное имя сервера) | `fake` |
| `NUDGE_COMPUTER_KILL` | both | Аварийный выключатель: значение `1` немедленно блокирует любые действия с компьютером | `0` |
| `NUDGE_COMPUTER_DRIFT` | both | При значении `1` считывает состояние заново при replay и формирует диффы drift | `0` |
| `NUDGE_AGENT_RUNTIME` | native | Путь к собранному бинарному файлу `nudge-agent` для команды `nudgec build --standalone` | Соседний бинарник |
| `NUDGE_CREDENTIALS_PATH` | native | Пользовательский путь к файлу хранилища учетных данных Nudge | `~/.nudge/credentials.json` |
| `NUDGE_OTEL` | python | Файл экспорта спанов OpenTelemetry | Нет |
| `NUDGE_OTEL_ENDPOINT` | python | HTTP-эндпоинт для экспорта данных OpenTelemetry | Нет |

Параметр `native` применяется к среде выполнения Rust (`nudgec`/`nudge-runtime`), `python/TS` относится к генерируемым средам, а `both` относится к обоим семействам сред выполнения.

## Примеры использования в реальном мире

Ознакомьтесь с полноценными реализациями агентов в каталоге [`examples/`](examples/):

| Пример | Направление | Главные особенности |
|:---|:---|:---|
| [AI Chatbot](examples/chatbot/) | Разговорный ассистент | Типизированные схемы памяти, детерминированный replay, лимиты budget |
| [Triage Agent](examples/triage-agent/) | Классификация обращений | Пакетные блоки `decide{}`, пороги confidence, политики маршрутизации |
| [Code Reviewer](examples/code-reviewer/) | Программная инженерия | Структурированная диагностика синтаксиса, автоматическая оценка по рубрикам |
| [Research Agent](examples/research_agent.ndg) | Анализ нескольких источников | Параллельный сбор данных с `par map`, расчет confidence |
| [Computer Automation](examples/computer-automation/) | Автоматизация рабочего стола | Наблюдение за экраном, деревья доступности, валидация drift |
| [RAG Agent](examples/rag-agent/) | Retrieval-Augmented Generation | Привязка инструментов извлечения MCP через stdio, извлечение цитат |
| [Property Fuzzer](examples/property-fuzz/) | Тестирование агентов | Инвариантное тестирование с `for_all`, фаззинг prompt injection |
| [Dungeon Crawler](examples/dungeon-crawler/) | Симуляция конечного автомата | Чистый Nudge, переходы между состояниями агента, пошаговый бой |

## Лицензия и сборка из исходников

Nudge распространяется под лицензией с открытым исходным кодом **Apache-2.0**. Подробности в файле [LICENSE](LICENSE).

### Сборка из исходников

Для сборки компилятора требуется стабильная версия Rust 1.75+:

```sh
git clone https://github.com/NekomyaDev/nudge.git
cd nudge
cargo build --release -p nudgec   # compiler binary at target/release/nudgec
cargo test --workspace            # compiler, VM, and runtime test suite
```

Среда выполнения Python находится в папке `runtime/` (только стандартная библиотека, без внешних зависимостей):

```sh
pip install ./runtime
```

---

<p align="center">
  Made with love by <a href="https://github.com/NekomyaDev">NekomyaDev</a>
</p>
