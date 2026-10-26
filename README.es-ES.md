<p align="center">
  <img src="assets/logo.svg" width="200" height="200" alt="Nudge Logo">
</p>

<h1 align="center">Nudge</h1>

<p align="center">
  <strong>No intentes parsear a tus agentes. Dales un Nudge.</strong><br>
  Un lenguaje de programación tipado, reproducible y consciente del presupuesto para agentes de IA.<br>
  Runtime de bytecode nativo y compilador independiente. Compila a Python y TypeScript.
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

## ¿Por qué Nudge?

Los agentes en producción todavía se sostienen con código de unión frágil: cadenas de prompts parseadas a mano, llamadas a tools envueltas en bloques try/except, falta de replayability, costes impredecibles y ausencia total de pruebas de regresión. Las librerías solo ponen parches a los síntomas una vez que ocurren. **Nudge soluciona el problema en la capa donde realmente vive: el lenguaje.**

| Punto Crítico | Librerías | Nudge |
|:---|:---|:---|
| Salida de LLM sin tipar | Validar en runtime | El schema es un tipo de primera clase en el lenguaje, verificado en tiempo de compilación |
| Efectos secundarios ocultos | Invisibles | 10 effects tipados y explícitos declarados en las firmas (`uses LLM, Tool, IO, ...`) |
| Sin pruebas de regresión | Record y replay añadidos como parche | Cada ejecución escribe una traza NTF, y cada trace es un test determinista de $0 |
| Costes imprevistos | Monitoreados tras recibir la factura | El budget es un contrato estricto impuesto por el compilador y el runtime |
| Espagueti de concurrencia | Bucles asíncronos manuales | `par map / race / all` nativo con seguridad frente a carreras en tiempo de compilación |

## Un vistazo a Nudge

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

El compilador demuestra que el schema coincide, infiere los effects y acota el coste en el peor de los casos. El runtime registra cada llamada en una traza direccionada por contenido que puedes comparar con diff, confirmar en git y reproducir por $0.

## Inicio rápido

### 1. Instalar Nudge

**Instalador de una sola línea (Recomendado):**

```sh
# Linux / macOS
curl -fsSL https://raw.githubusercontent.com/NekomyaDev/nudge/main/install.sh | bash

# Windows (PowerShell as Administrator)
irm https://raw.githubusercontent.com/NekomyaDev/nudge/main/install.ps1 | iex
```

**Gestores de paquetes y contenedores:**

```sh
# Snap (Linux)
sudo snap install nudge --classic

# Docker
docker run -it --rm -v $(pwd):/workspace nekomyadev/nudge nudgec --help
```

Los binarios precompilados para Linux, macOS y Windows también están disponibles en la página de [Releases](https://github.com/NekomyaDev/nudge/releases).

### 2. Primera ejecución en 60 segundos (Nativo, $0)

Sin necesidad de Python, Node.js ni claves de API. Nudge incluye una VM de pila nativa y un proveedor simulado determinista en memoria:

```sh
# Crear un programa
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

# Verificar tipos del programa
nudgec check hello.ndg

# Ejecutar directamente en la VM de Bytecode nativa ($0, sin claves)
nudgec run hello.ndg

# Ejecutar pruebas de regresión offline
nudgec test hello.ndg
```

### 3. Lecciones interactivas y plantillas de proyecto

```sh
# Seis lecciones interactivas en tu terminal (hello, types, decide, policy, test, trace)
nudgec learn

# Crear un proyecto desde plantillas listas para usar (triage, chatbot, rag-agent, etc.)
nudgec init my-agent --template triage
cd my-agent && nudgec check my-agent.ndg && nudgec run my-agent.ndg
```

### 4. Compilar a Python o TypeScript (Opcional)

Si estás desplegando en un servicio existente en Python o TypeScript:

```sh
# Target de Python
pip install nudge-runtime          # standard library only, zero dependencies
nudgec build hello.ndg              # emits out/hello.py
python3 out/hello.py

# Target de TypeScript
nudgec build-ts hello.ndg             # emits out/hello.ts
```

## Características principales de un vistazo

| Característica | Qué ofrece | Sección |
|:---|:---|:---|
| **Llamadas LLM tipadas** | El schema de salida es un tipo y las violaciones disparan auto-reparación | [Un vistazo a Nudge](#un-vistazo-a-nudge) |
| **Flujo de control y tipos** | `if/else`, bucles `for`, literales de registro `{k: v}`, indexación, variables locales mutables, Opcional (`T?`), Unión (`T \| U`) | [Flujo de control y datos](#flujo-de-control-y-datos) |
| **Effects tipados** | 10 effects conocidos rastreados y comprobados en tiempo de compilación | [Effects tipados](#effects-tipados) |
| **Workspace y Store** | Sistema de archivos en sandbox acotado + almacenamiento clave-valor persistente tipado | [Workspace y Store](#workspace-acotado-y-store-persistente) |
| **UI local** | Vistas tipadas, ventanas, tests headless y ventana de escritorio nativa del SO | [UI local](#ui-local) |
| **Tareas y concurrencia** | Spawn/join de tareas, cancelación cooperativa, `par race` seguro, liberación de budget | [Ciclo de vida de tareas y concurrencia](#ciclo-de-vida-de-tareas-y-concurrencia) |
| **Autenticación de proveedores** | Flujo de dispositivo RFC 8628 OAuth fail-closed, sin recolección de tokens ajenos | [Autenticación de proveedores](#autenticación-de-proveedores) |
| **Decisiones tipadas** | Clasificación por lotes, sí/no, puntuación por rúbricas sobre estados (`/v1/systemone`) | [Decisiones tipadas](#decisiones-tipadas) |
| **Computer Use** | Observación de pantalla, acciones tipadas, lista blanca de apps, kill switch | [Computer Use](#computer-use) |
| **VM de Bytecode** | Motor predeterminado de VM de pila (~70x más rápido en cómputo puro, recursión profunda segura) | [Motor de VM de Bytecode](#motor-de-vm-de-bytecode) |
| **Agentes independientes** | Empaquetado en un único binario sin dependencias de Cargo (~2.09MB en vivo, ~832KB replay) | [Agentes nativos independientes (sin Cargo)](#agentes-nativos-independientes-sin-cargo) |
| **Target WebAssembly** | Replay offline en WASI (~526KB), puente para host de tools en vivo | [Target WebAssembly](#target-webassembly) |
| **Trazas deterministas** | Trazas abiertas NTF v1, replay por $0, puertas de regresión en CI, diff de trazas | [Trazas deterministas y CI para agentes](#trazas-deterministas-y-ci-para-agentes) |
| **Redacción de privacidad** | Auto-redacción predeterminada de secretos y PII en un único punto de control | [Privacidad y redacción en trazas](#privacidad-y-redacción-en-trazas) |

## Lenguaje y capacidades

### Flujo de control y datos

Nudge proporciona construcciones familiares de programación estructurada con límites de seguridad deterministas:

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

- **Bifurcaciones:** expresiones `if/else` y `else if` compatibles con bloques de múltiples sentencias.
- **Iteración:** bucles secuenciales `for item in coll` y bucles indexados `for (item, idx) in coll`.
- **Datos estructurados:** literales de registro `{ field1: expr1, field2: expr2 }` con tipado estático y acceso a campos (`obj.field`).
- **Indexación:** acceso indexado en colecciones y registros (`items[0]`, `map["key"]`). Los errores de límites fallan de forma limpia con `E0705`.
- **Variables locales mutables:** variables explícitas con `let mut name = init` y reasignación (`name = next`).
- **Límites de bucles:** los bucles están limitados por `NUDGE_MAX_LOOP_ITERATIONS` (por defecto 10.000) para evitar ejecuciones descontroladas, emitiendo `E0703` si se supera el límite.
- **Tipos opcionales (`T?`):** tipos anulables representados con `?` (como `string?`, `int?`, `(int | string)?`), donde los valores ausentes toman `none`. Las comprobaciones de igualdad usan `== none` y `!= none`. El uso no comprobado falla cerrado con `E0201`.
- **Tipos de unión (`T | U`):** tipos estáticos de unión alternativa (como `int | string` o `string | bool`). Admite comprobaciones de ensanchamiento en tiempo de compilación y rechazo estricto de estrechamiento (`E0201`). Totalmente compatible con esquemas estructurados de LLM, serialización JSON y generación de código.

### Effects tipados

Cada efecto secundario se declara en la firma de la función y es validado por el compilador:

```nudge
fn run_pipeline() -> bool uses LLM, Tool, Workspace, Store {
    // Calling unlisted effects triggers compiler error E0301
}
```

Nudge define 10 effects conocidos (`crates/nudgec/src/check.rs`):
- `LLM`: Invoca completados de modelos de lenguaje o prompts en streaming.
- `Tool`: Invoca herramientas MCP externas o registradas.
- `IO`: Operaciones de entrada/salida del host (`print`, `println`).
- `Decision`: Consulta modelos de decisión mediante `decide{}`.
- `Computer`: Interactúa con entornos de escritorio y árboles de accesibilidad mediante `computer.*`.
- `Workspace`: Operaciones acotadas de lectura y escritura dentro del workspace en sandbox.
- `WorkspaceDelete`: Eliminación destructiva de archivos o directorios dentro del workspace.
- `Store`: Operaciones de almacenamiento clave-valor persistente mediante `store.*`.
- `UI`: Crea, monta o despacha eventos a elementos de interfaz gráfica de usuario.
- `Auth`: Accede a recursos autenticados del proveedor (admite `Auth(account)` con ámbito).

Las funciones sin cláusula `uses` se demuestran puras en tiempo de compilación. Los effects desconocidos se rechazan con `E0302`.

### Workspace acotado y Store persistente

Los agentes locales deben trabajar con archivos y mantener el estado entre ejecuciones sin comprometer la seguridad del sistema:

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

- **Workspace en sandbox (`workspace.*`):** todas las operaciones están confinadas a una raíz de workspace explícita (configurada mediante `--workspace` o `NUDGE_WORKSPACE_DIR`, por defecto el directorio actual). Se bloquean los escapes fuera de la raíz como path traversals (`../`) y enlaces simbólicos. Las escrituras son consistentes ante caídas (escritura atómica en archivo temporal y renombrado).
- **Separación de privilegios de borrado:** borrar archivos o directorios requiere el effect dedicado `WorkspaceDelete` (`uses WorkspaceDelete`), impidiendo que rutinas generales de escritura puedan eliminar directorios.
- **Almacenamiento clave-valor persistente (`store.*`):** almacenamiento fuertemente tipado guardado en `<workspace>/.nudge/store.json` (o `NUDGE_STORE_PATH`). A diferencia de los puntos de control efímeros de ejecución, este almacén persiste entre distintas ejecuciones y reinicios del sistema.

### Monitorización de procesos y archivos

Construye agentes que controlen ejecutores de compilación/tests, language servers y herramientas de recarga en vivo con la misma disciplina de effects y permisos que todo lo demás:

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

- **Monitorización de archivos (`fs.watch`, effect `FsWatch`):** monitoriza una ruta del workspace en busca de eventos de cambio, acotada por un tiempo de expiración (timeout, nunca se cuelga indefinidamente) y confinada a la raíz del workspace (escapes denegados). El uso no declarado es detectado por el sistema de tipos (`E0301`).
- **Procesos controlados (`proc.*`, effect `Process`):** `proc.spawn(cmd, argv, timeout_ms)` / `proc.run(...)` (spawn + await) / `proc.await` / `proc.kill` / `proc.alive`. El comando `cmd` y sus argumentos `argv` se pasan por **separado** y se ejecutan sin shell (eliminando la superficie de inyección en shell). Su ciclo de vida es acotado (`E0921`) sin **procesos huérfanos** (los hijos se limpian al salir o cancelar la tarea) y con terminación kill-on-cancel. La ejecución es **fail-closed**: denegada por defecto a menos que esté autorizada por `NUDGE_PROCESS_GRANTS` (`E0920`, confinamiento al workspace `E0922`).

### UI local

Crea interfaces nativas con interacción humana en el bucle (human-in-the-loop), estaciones de aprobación y paneles de inspección interactivos directamente en Nudge:

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

- **Ejecución en modo dual:**
  - **Pruebas Headless:** `mount_headless()` y `dispatch_click()` evalúan árboles de UI y transiciones de estado enteramente en memoria sin abrir una ventana física. Se ejecutan en CI por $0.
  - **GUI de escritorio nativa:** al compilar con la feature de Cargo `native-gui`, `window.open()` crea una ventana nativa del sistema operativo impulsada por `winit` y `softbuffer`.
- **Modo headless automático:** configurar `NUDGE_UI_HEADLESS=1` o ejecutar durante el replay de una traza (`NUDGE_REPLAY`) redirige automáticamente las llamadas de ventana al motor headless.

### Ciclo de vida de tareas y concurrencia

Coordina ramas de agentes concurrentes con gestión formal de estado de tareas y cancelación cooperativa:

```nudge
fn race_models(prompt: string) -> string uses LLM {
    // par race automatically cancels losing branches
    par race {
        fast: query_fast_model(prompt),
        deep: query_deep_model(prompt)
    }
}
```

- **Cancelación cooperativa:** cuando un `race` se resuelve, las ramas perdedoras se detienen en los límites de bucles, streams y effects en lugar de seguir consumiendo budget en segundo plano.
- **Protección de presupuesto:** las reservas de budget no gastadas bajo `NUDGE_BUDGET` se liberan inmediatamente al cancelar la tarea.
- **Permisos fail-closed:** las tareas canceladas pierden inmediatamente el permiso para despachar más llamadas a tools (`Tool`) o acciones de computadora (`Computer`).
- **Primitivas de tareas:** control explícito de tareas mediante `task.spawn`, `task.join`, `task.status` y `task.cancel`.

### Autenticación de proveedores

Gestión segura de credenciales integrada directamente en el lenguaje y el runtime:

- **Autenticación fail-closed:** las funciones declaran requisitos de autenticación explícitos con `uses Auth` o `uses Auth(account)`. Las llamadas no autenticadas fallan cerrando el acceso.
- **Sin recolección de tokens ajenos:** Nudge nunca lee cachés privadas de tokens ni almacenes de credenciales pertenecientes a herramientas de terceros.
- **Flujo de dispositivo nativo RFC 8628:** autorización interactiva en terminal mediante OAuth 2.0 Device Authorization Grant con almacenamiento seguro de credenciales en `~/.nudge/credentials.json` (permisos de archivo 0600).
- **Puertas de seguridad por proveedor:** los proveedores deben satisfacer contratos de seguridad verificables (`E-auth-0401` para credenciales faltantes, `E-auth-0403` para déficits de scope, `E-auth-0404` para proveedores desactivados).
- **Proveedores no comprobados permanecen cerrados (puerta de permisos):** un proveedor cuyo permiso oficial de uso por terceros no haya sido demostrado es **fail-closed por defecto**. Esto se aplica actualmente a `antigravity`: su flujo de login OAuth y su código de proveedor existen, pero dado que no se encontró prueba oficial de que su cliente OAuth y su endpoint de modelo puedan utilizarse desde un cliente independiente de terceros, la puerta permanece **desactivada** (`E-auth-0404`) hasta que se verifique el permiso. **Una prueba mock de OAuth o un pase exitoso en CI NO demuestran acceso a cuentas reales ni permiso de uso.** Los consumidores (incluida cualquier aplicación construida sobre Nudge) no deben depender de esta conexión hasta que el permiso esté documentado. Una habilitación explícita asumida por el usuario (`NUDGE_ENABLE_ANTIGRAVITY=1`) la activa bajo la propia responsabilidad del usuario una vez verificado el permiso.

### Decisiones tipadas

Compila la incertidumbre procedente de modelos de decisión (como Laya y Jev) mediante el protocolo `/v1/systemone`:

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

- **Ejecución por lotes:** una sola llamada evalúa todas las preguntas simultáneamente, devolviendo ganador, probabilidad, distribución y confianza.
- **Pruebas por cero dólares:** el proveedor simulado predeterminado sintetiza distribuciones deterministas con semilla para ejecuciones de prueba por $0.
- **Proveedores en vivo:** conecta modelos locales o remotos mediante `NUDGE_DECISION_SERVERS='{"laya": {"base_url": "http://localhost:8000"}}'`.
- **Caché de decisiones:** `NUDGE_DECISION_CACHE=<ruta>` almacena en caché las respuestas validadas entre ejecuciones para no pagar dos veces por el mismo estado.
- **Barridos de políticas (Policy Sweeps):** ejecuta `nudgec policy-sweep trace.jsonl --question dept --thresholds 0.5,0.8` para evaluar ajustes de umbrales sobre distribuciones registradas sin realizar llamadas a modelos. Consulta la [Documentación sobre Decisiones](docs/decision.md).

### Computer Use

Controla entornos de escritorio con observaciones y acciones tipadas donde el lenguaje gobierna el control y la seguridad mientras el modelo gestiona la percepción:

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

- **Primitivas tipadas:** `computer.observe` devuelve un registro `Observation` (árbol de accesibilidad, elementos, captura de pantalla opcional). Las acciones (`click`, `type`, `key`, `scroll`, `set_value`, `drag`, `paste`) devuelven `ActionResult` con acuses de recibo de despacho (`action_sent`).
- **Seguridad en replay:** el replay de trazas nunca vuelve a ejecutar acciones físicas. Con `NUDGE_COMPUTER_DRIFT=1`, el runtime vuelve a observar el estado en vivo y calcula un diff mecánico de desviación (`Drift`) sin disparar acciones.
- **Protección y endurecimiento:** lista blanca de aplicaciones (`allow: [app]`), bloqueos de concesión de controlador, rechazo de índices de elementos obsoletos (`stale_state`) y un interruptor de apagado de emergencia (kill switch) estricto (`NUDGE_COMPUTER_KILL=1`).
- **Escritorio determinista:** el proveedor simulado predeterminado ejecuta escenarios de UI en memoria (`NUDGE_COMPUTER_SCENARIO`), permitiendo pruebas integrales por $0. Consulta la [Documentación de Computer Use](docs/computer-use.md).

## Infraestructura nativa

### Motor de VM de Bytecode

Las funciones puras, las transformaciones de datos y las bifurcaciones de rutas se ejecutan en una máquina virtual de bytecode basada en pila dedicada (`crates/nudgec/src/bytecode.rs` y `vm.rs`):

- **Cómputo puro de alta velocidad:** ejecuta código puro aproximadamente 70 veces más rápido que el intérprete sobre AST.
- **Seguridad ante recursión profunda:** la pila de frames gestionada elimina los desbordamientos de pila del sistema operativo en algoritmos recursivos.
- **Delegación transparente de effects:** las expresiones con efectos (`llm`, `tool`, `computer`, `workspace`) delegan de forma transparente desde la VM al evaluador del runtime mediante `DelegateCall` y `DelegateExpr`.
- **Motor predeterminado:** la VM se ejecuta por defecto en `nudgec run` y binarios independientes. Configura `NUDGE_VM=0` para volver al intérprete basado en AST.

### Agentes nativos independientes (sin Cargo)

Empaqueta tu agente en un binario ejecutable nativo único y autónomo sin dependencias de toolchain en el entorno de despliegue:

```sh
# Package an agent into a standalone binary
nudgec build --standalone my-agent.ndg -o bin/my-agent

# Run directly on the host (zero Python/Node/Rust dependencies)
./bin/my-agent
```

- **Estampado de AST:** `nudgec` comprueba los tipos del código fuente, serializa el conjunto del AST en bincode y lo añade a un binario de runtime precompilado de `nudge-agent` (pie de página `NDGAGNT1`) en cuestión de milisegundos.
- **Dos variantes de runtime:**
  - **Completo (~2.09MB, predeterminado):** runtime con soporte en vivo y pila HTTP/TLS integrada con `ureq` + `rustls` para LLMs y servidores MCP en vivo.
  - **Solo Replay (~832KB):** runtime compacto sin dependencias de red, resolviendo todas las operaciones desde trazas NTF grabadas para ejecuciones offline por $0.
- **Detección del runtime:** se localiza mediante la variable de entorno `NUDGE_AGENT_RUNTIME` o como un binario hermano junto a `nudgec`. Consulta la [Documentación de Standalone](docs/standalone.md).

### Target WebAssembly

Despliega agentes en sandboxes de navegador, runtimes perimetrales (Cloudflare Workers, Fastly) y entornos WASI:

- **Replay offline (`nudge-replay-wasi`):** reproduce trazas NTF registradas de forma offline dentro de un módulo `.wasm` de aproximadamente 526KB bajo `wasmtime` con una salida idéntica byte a byte a las ejecuciones nativas:
  ```sh
  wasmtime run --dir . nudge-replay-wasi.wasm bundle.json trace.jsonl
  ```
- **Puente para host de tools en vivo:** los agentes en JavaScript en navegadores o entornos edge realizan llamadas a tools en vivo mediante el contrato de importación estandarizado `nudge_host_tool_call` (verificado bajo Node WASI, ver `examples/wasm-host/`).
- **Seguridad en sandbox:** los sockets de red y la creación de procesos del sistema operativo quedan deshabilitados en `wasm32`, garantizando una ejecución aislada. Consulta la [Documentación de WebAssembly](docs/wasm.md).

## Herramientas e integraciones

### Integración MCP

Declara tools respaldadas por servidores Model Context Protocol (MCP) que se comunican mediante stdio:

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

Configura servidores en vivo mediante `NUDGE_MCP_SERVERS`:

```sh
export NUDGE_MCP_SERVERS='{"search": {"command": "mcp-server-websearch"}}'
nudgec run search_agent.ndg
```

Los parámetros se pasan como objetos de argumentos JSON con nombre para compatibilidad con FastMCP y servidores estándar. Los servidores no disponibles fallan rápidamente con diagnósticos claros.

### Trazas deterministas y CI para agentes

Cada ejecución genera una traza JSONL abierta en formato NTF v1 capturando todas las interacciones de los effects:

```sh
# Verify trace integrity against the open specification
nudgec trace-check trace.jsonl

# Compare two traces to identify cost or behavior changes (--json emite informes procesables por máquina)
nudgec trace-diff baseline.jsonl candidate.jsonl --fail-on-regression --json
```

**GitHub Action (`action.yml`):** prueba agentes en CI en cada push con coste $0 y sin claves de API:

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

La especificación congelada de NTF se encuentra en [docs/ntf-spec.md](docs/ntf-spec.md), con una suite de pruebas completa en [`conformance/`](conformance/) y puentes para LangChain, CrewAI y LangGraph.

### Herramientas para desarrolladores

- **Estimador de coste estático:** `nudgec cost <file.ndg> [--json]` calcula en tiempo de compilación los costes de tokens y presupuestos en el peor de los casos por función.
- **Pruebas de propiedades y fuzzing:** `nudgec fuzz [file.ndg] [--iterations N] [--seed S] [--json]` ejecuta pruebas de fuzzing basadas en mutaciones y verificación de propiedades.
- **Evaluación de conjuntos de datos:** `nudgec eval <file.ndg> --dataset <rows.jsonl> [--fn <name>] [--path <dotted>] [--min-accuracy 0.8] [--json]` evalúa funciones frente a datasets JSONL y bloquea CI según la precisión requerida.
- **Comparación de evaluaciones:** `nudgec compare <results_a.jsonl> <results_b.jsonl> [--json]` compara dos ejecuciones de evaluación para detectar regresiones de precisión o rendimiento.
- **Explicador de ejecución:** `nudgec explain <trace.jsonl> [--json]` genera un resumen CLI legible para humanos o un análisis JSON detallando fallos, consumo de tokens y decisiones de baja confianza.
- **Diferencias de trazas:** `nudgec trace-diff <a.jsonl> <b.jsonl> [--json] [--fail-on-regression]` compara dos trazas para identificar regresiones, saliendo con código 1 ante regresiones en CI.
- **Auditoría de cumplimiento:** `nudgec audit <trace.jsonl> [--json]` audita una traza NTF según los requisitos de la Ley de IA de la UE (transparencia, gobernanza, secretos no redactados, métricas de riesgo).
- **Visor de trazas local:** `nudgec trace-view <trace.jsonl> [--port N] [--no-open]` inicia una interfaz web local con líneas de tiempo de ejecución visuales, gasto de tokens y detalles de reparaciones.
- **Formateador y watcher:** `nudgec fmt <file.ndg> [--check] [--json]` formatea el código con seguridad y `nudgec check <file.ndg> [--json] --watch` comprueba los tipos en cada guardado.
- **Linter y autocorrección:** `nudgec lint <file.ndg> [--fix] [--json]` revisa el código en busca de buenas prácticas (presupuestos ilimitados, prompts ambiguos, esquemas sin reparación) y aplica correcciones seguras.
- **Inspector de almacén de ejecuciones:** `nudgec runs [--run <run_id>] [--json]` inspecciona estados de ejecución persistentes, trazas y puntos de control de agentes.
- **Gestor de autenticación:** `nudgec auth login [provider]` y `nudgec auth status [--json]` gestionan credenciales OAuth y verifican proveedores de IA conectados.
- **Exportación HTML estática:** `nudgec trace-html <trace.jsonl> [-o report.html]` exporta informes interactivos de auditoría completamente autónomos.

### Extensión para VS Code

Instala la [Extensión de Lenguaje Nudge](https://marketplace.visualstudio.com/items?itemName=Nekomya.nudge-lang) para resaltado de sintaxis, fragmentos de código (snippets), documentación al pasar el cursor y diagnósticos de tipos en tiempo real impulsados por `nudgec lsp`.

## Privacidad y redacción en trazas

Los agentes de IA manejan con frecuencia tokens sensibles, cabeceras de autorización, claves privadas y datos de identificación personal (PII). Nudge protege tus datos mediante una canalización de redacción de trazas fail-closed:

- **Punto único de control:** la redacción se realiza dentro de `TraceWriter::emit` antes de serializar los registros en disco o en búferes de memoria. Todos los tipos de registro (`llm.call`, `tool.call`, `decision.call`, `computer.*`, `fn.return`) se enmascaran en este cuello de botella centralizado.
- **Fail-closed por defecto:** la redacción de trazas está activa por defecto (`NUDGE_TRACE_REDACT=1`). Establece `NUDGE_TRACE_REDACT=0` únicamente para depuración local explícita.
- **`NUDGE_TRACE_REDACT=0` es un límite de seguridad, no una conveniencia:** desactivar la redacción graba secretos, claves de API, cabeceras de autorización y datos personales en **TEXTO PLANO** en el archivo de traza. Esta es la consecuencia documentada e intencionada de desactivarlo (respaldada por tests). Nunca lo uses en CI, con artefactos compartidos o confirmados en repositorios, ni cuando una app construida sobre Nudge consuma trazas. Los consumidores deben tratar la presencia de `NUDGE_TRACE_REDACT=0` como un parámetro crítico de seguridad y NO deben asumir que las trazas sean seguras de almacenar o compartir en ese modo.
- **Detección automática de secretos:** identifica y enmascara automáticamente variables de entorno que terminen en `_KEY`, `_SECRET`, `_TOKEN`, `_PASSWORD`, `_PAT`, o que contengan `AUTH` (con valores de 6 o más caracteres).
- **Enmascaramiento de patrones por capas:** limpia tokens bearer, cabeceras de autorización, pares credencial clave-valor, bloques de claves privadas y prefijos de tokens de proveedores (OpenAI `sk-...`, Anthropic `sk-ant-...`, GitHub `ghp_...`, AWS `AKIA...`, Slack `xox...`).
- **Invarianza criptográfica en replay:** el `request_hash` direccionado por contenido se calcula sobre las entradas **previas a la redacción**. Esto garantiza que las trazas enmascaradas sigan siendo idénticas byte a byte durante la búsqueda en el replay con coincidencia O(1) sin persistir credenciales en disco.

## Variables de entorno

| Variable | Ámbito | Propósito | Valor por defecto |
|:---|:---|:---|:---|
| `NUDGE_PROVIDER` | ambos | Proveedor de LLM activo (`fake`, `openai`, `anthropic`, `gemini`, `groq`, `ollama`) | `fake` |
| `NUDGE_VM` | nativo | Interruptor del motor de ejecución VM de Bytecode (`1` = VM por defecto, `0` = fallback a intérprete AST) | `1` |
| `NUDGE_DEFAULT_MODEL` | nativo | Cadena del modelo predeterminado si no se especifica explícitamente en el bloque `with` | Predeterminado del proveedor |
| `NUDGE_API_KEY` / `NUDGE_BASE_URL` | ambos | Credenciales y endpoint base para proveedores de LLM compatibles con OpenAI | Ninguno |
| `NUDGE_PRICING` | ambos | Tabla personalizada de precios en JSON: `{"model": [usd_per_1M_in, usd_per_1M_out]}` | Tabla integrada |
| `NUDGE_GUARD` | python/TS | Guardas de salida (`pii` enmascara secretos, correos electrónicos y direcciones IP en salidas de LLM) | Ninguno |
| `NUDGE_BUDGET` | ambos | Límite máximo de gasto en USD a nivel de ejecución para despliegues en paralelo | Ninguno |
| `NUDGE_REPAIR_BUDGET` | python | Límite máximo acumulado en USD para rondas de reparación | Ninguno |
| `NUDGE_MAX_LOOP_ITERATIONS` | nativo | Límite máximo de iteraciones secuenciales de bucle antes de generar el error de runtime `E0703` | `10000` |
| `NUDGE_TRACE` | ambos | Ruta de destino para escribir la traza de ejecución NTF JSONL | `trace.jsonl` |
| `NUDGE_REPLAY` | ambos | Ruta de la traza para replay offline determinista (`NUDGE_REPLAY_MODE=all` o `llm`) | Ninguno |
| `NUDGE_RESUME` | ambos | Reanudar ejecución tras fallo desde un punto de control usando el prefijo de traza grabado | Ninguno |
| `NUDGE_TRACE_REDACT` | nativo | Activa el punto único de enmascaramiento de secretos y PII en trazas (`1` o `0`) | `1` (Activo) |
| `NUDGE_TRACE_REDACT_ENV` | nativo | Enmascara automáticamente en las trazas los valores de variables de entorno con secretos | `1` (Activo) |
| `NUDGE_TRACE_REDACT_KEYS` | nativo | Lista separada por comas de claves JSON adicionales a enmascarar | Ninguno |
| `NUDGE_TRACE_REDACT_PATTERNS` | nativo | Lista de regex separada por comas o ruta a un archivo de regex para enmascaramiento personalizado | Ninguno |
| `NUDGE_WORKSPACE_DIR` | nativo | Directorio raíz con autoridad para operaciones acotadas de `workspace.*` | Directorio actual |
| `NUDGE_STORE_PATH` | nativo | Ubicación del archivo de almacenamiento para datos tipados persistentes clave-valor | `<workspace>/.nudge/store.json` |
| `NUDGE_UI_HEADLESS` | nativo | Fuerza a que las operaciones de ventana de UI se ejecuten de forma headless en memoria (`1` o `true`) | `0` |
| `NUDGE_TOOL_GRANTS` | ambos | Política de tools en JSON para la capa de ejecución (ej. `{"web_search": ["*"]}`). Falla cerrando | Ninguno |
| `NUDGE_PROCESS_GRANTS` | nativo | Política fail-closed en JSON para comandos y argumentos de `proc.*` | Ninguno (denegado) |
| `NUDGE_PROCESS_TIMEOUT` | nativo | Tiempo de expiración predeterminado en milisegundos para procesos hijos en `proc.*` | `30000` |
| `NUDGE_FS_WATCH_TIMEOUT` | nativo | Tiempo de expiración y espera predeterminado en milisegundos para `fs.watch` | `30000` |
| `NUDGE_ENABLE_ANTIGRAVITY` | nativo | Interruptor de activación explícito para el proveedor Antigravity (`1` o `true`) | `0` |
| `NUDGE_AUTH_ANTIGRAVITY` | nativo | Token bearer directo para autenticación con el proveedor Antigravity | Ninguno |
| `NUDGE_AUTH_GITHUB_COPILOT` | nativo | Sustitución directa de token para autenticación con GitHub Copilot | Ninguno |
| `NUDGE_MCP_SERVERS` | ambos | Registro JSON que mapea nombres de servidores MCP a comandos stdio | Ninguno |
| `NUDGE_MCP_TIMEOUT_MS` | nativo | Límite de tiempo de lectura por solicitud y timeout de sesiones colgadas para MCP | `30000` |
| `NUDGE_DECISION_SERVERS` | ambos | Registro JSON para endpoints de modelos de decisión (ej. `{"laya": {"base_url": "..."}}`) | Ninguno |
| `NUDGE_DECISION_CACHE` | ambos | Ruta del archivo de caché para persistir distribuciones de modelos de decisión | Ninguno |
| `NUDGE_COMPUTER_SERVERS` | ambos | Registro JSON para puentes de computer use (subproceso JSONL o HTTP) | Ninguno |
| `NUDGE_COMPUTER_PROVIDER` | ambos | Puente seleccionado para acciones de computadora (`fake`, o nombre del servidor registrado) | `fake` |
| `NUDGE_COMPUTER_KILL` | ambos | Interruptor estricto de parada: `1` rechaza inmediatamente toda operación de computadora | `0` |
| `NUDGE_COMPUTER_DRIFT` | ambos | Con `1`, vuelve a observar el estado en vivo durante el replay y adjunta diffs mecánicos de desviación | `0` |
| `NUDGE_AGENT_RUNTIME` | nativo | Ruta al binario precompilado de `nudge-agent` para `nudgec build --standalone` | Binario hermano |
| `NUDGE_CREDENTIALS_PATH` | nativo | Ruta personalizada al almacén de credenciales de autenticación de Nudge | `~/.nudge/credentials.json` |
| `NUDGE_OTEL` | python | Archivo de exportación de trazas OpenTelemetry | Ninguno |
| `NUDGE_OTEL_ENDPOINT` | python | Endpoint de exportación HTTP para OpenTelemetry | Ninguno |

`nativo` aplica al runtime de Rust (`nudgec`/`nudge-runtime`), `python/TS` a los runtimes generados heredados, y `ambos` a ambas familias de runtime.

## Ejemplos del mundo real

Explora implementaciones completas de agentes en [`examples/`](examples/):

| Ejemplo | Dominio | Aspectos destacados |
|:---|:---|:---|
| [AI Chatbot](examples/chatbot/) | Asistente conversacional | Schemas de memoria tipados, replay determinista, techos de budget |
| [Triage Agent](examples/triage-agent/) | Clasificación de soporte | Bloques `decide{}` por lotes, umbrales de confianza, políticas de enrutamiento |
| [Code Reviewer](examples/code-reviewer/) | Ingeniería de software | Diagnósticos de sintaxis estructurados, puntuación automatizada por rúbricas |
| [Research Agent](examples/research_agent.ndg) | Síntesis multifuente | Recuperación paralela con `par map`, puntuación de confianza |
| [Computer Automation](examples/computer-automation/) | Agente de escritorio | Observación de pantalla, árboles de accesibilidad, validación de drift |
| [RAG Agent](examples/rag-agent/) | Generación aumentada por recuperación | Enlaces a herramientas MCP stdio de recuperación, extracción de citas |
| [Property Fuzzer](examples/property-fuzz/) | Pruebas de agentes | Pruebas de invariantes `for_all`, fuzzing de inyección de prompts |
| [Dungeon Crawler](examples/dungeon-crawler/) | Simulación de máquina de estados | Nudge puro, transiciones de estado de agentes, combate por turnos |

## Licencia y compilación desde el código fuente

Nudge es código abierto bajo la licencia **Apache-2.0**. Consulta [LICENSE](LICENSE) para más detalles.

### Compilación desde el código fuente

Compilar el compilador requiere Rust 1.75+ (versión estable):

```sh
git clone https://github.com/NekomyaDev/nudge.git
cd nudge
cargo build --release -p nudgec   # binario del compilador en target/release/nudgec
cargo test --workspace            # suite de pruebas del compilador, VM y runtime
```

El runtime de Python se encuentra en `runtime/` (solo biblioteca estándar, cero dependencias externas):

```sh
pip install ./runtime
```

---

<p align="center">
  Hecho con amor por <a href="https://github.com/NekomyaDev">NekomyaDev</a>
</p>
