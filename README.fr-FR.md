<p align="center">
  <img src="assets/logo.svg" width="200" height="200" alt="Nudge Logo">
</p>

<h1 align="center">Nudge</h1>

<p align="center">
  <strong>Ne passez plus votre temps à parser vos agents. Donnez-leur un Nudge.</strong><br>
  Un langage de programmation typé, rejouable et soucieux du budget pour les agents IA.<br>
  Moteur bytecode natif et compilateur autonome. Compile vers Python et TypeScript.
</p>

<p align="center">
  <img alt="Version" src="https://img.shields.io/badge/version-1.5-blue">
  <img alt="License" src="https://img.shields.io/badge/license-Apache--2.0-green">
  <img alt="Platform" src="https://img.shields.io/badge/platform-Linux%20%7C%20macOS%20%7C%20Windows-green">
  <img alt="Target" src="https://img.shields.io/badge/target-Native%20%7C%20Python%20%7C%20TypeScript%20%7C%20WASM-yellow">
  <a href="https://marketplace.visualstudio.com/items?itemName=Nekomya.nudge-lang"><img alt="VS Code" src="https://img.shields.io/badge/VS%20Code-Nudge%20Language-007ACC?logo=visualstudiocode"></a>
  <a href="https://github.com/NekomyaDev/nudge/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22"><img alt="Good First Issues" src="https://img.shields.io/github/issues/NekomyaDev/nudge/good%20first%20issue?color=7057ff&label=good%20first%20issues"></a>
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

## Pourquoi Nudge ?

Les agents en production sont encore trop souvent assemblés avec du code de bricolage : chaînes de prompts découpées à la main, appels d'outils enveloppés dans des blocs try/except, absence de rejouabilité, coûts imprévisibles et aucun test de régression. Les bibliothèques ne font que masquer les symptômes. **Nudge corrige le problème à la racine : au niveau du langage.**

| Point Critique | Bibliothèques classiques | Nudge |
|:---|:---|:---|
| Sortie LLM non typée | Validation au runtime | Le schéma est un type natif de premier ordre, vérifié à la compilation |
| Effets de bord cachés | Invisibles | 10 effets typés explicites déclarés dans les signatures (`uses LLM, Tool, IO, ...`) |
| Aucun test de régression | Replay greffé artificiellement | Chaque exécution écrit une trace NTF déterministe rejouable pour 0$ |
| Surprises budgétaires | Découvertes à la facture | Le budget est un contrat strict garanti par le compilateur et le runtime |
| Spaghetti asynchrone | Boucles async manuelles | Primitives natives `par map / race / all` avec sûreté vérifiée à la compilation |

## Un aperçu de Nudge

```nudge
type Finding = { claim: string, source: Url, confidence: float @range(0, 1) }

fn analyze(q: string, hits: [SearchResult]) -> [Finding] uses LLM {
    llm"""Extrais des observations vérifiables sur {q} depuis : {hits}"""
    with { schema: [Finding], model: "anthropic:sonnet-4.6",
           budget: 0.03 USD, retry: 2 with repair }
}

test "reste dans le budget sur trace enregistrée" {
    let t = replay("traces/demo.jsonl")
    assert t.cost_usd < 0.25          // zéro token consommé en CI
}
```

Le compilateur garantit la conformité du schéma, déduit les effets et borne le coût maximal. Le runtime enregistre chaque interaction dans une trace adressée par son contenu, versionnable et rejouable pour 0$.

## Démarrage rapide

### 1. Installer Nudge

**Installateur en une ligne (Recommandé) :**

```sh
# Linux / macOS
curl -fsSL https://raw.githubusercontent.com/NekomyaDev/nudge/main/install.sh | bash

# Windows (PowerShell en Administrateur)
irm https://raw.githubusercontent.com/NekomyaDev/nudge/main/install.ps1 | iex
```

**Gestionnaires de paquets et conteneurs :**

```sh
# Snap (Linux)
sudo snap install nudge --classic

# Docker
docker run -it --rm -v $(pwd):/workspace nekomyadev/nudge nudgec --help
```

### 2. Premier test en 60 secondes (Natif, 0$)

Sans Python, sans Node.js et sans clés d'API requises :

```sh
# Créer un programme
cat > hello.ndg << 'EOF'
type Greeting = { message: string, timestamp: string }

fn greet(name: string) -> Greeting uses LLM {
    llm"""Crée une salutation pour {name}. Renvoie message et timestamp."""
    with { schema: Greeting, model: "anthropic:sonnet-4.6", budget: 0.01 USD }
}

fn main() -> Greeting uses LLM {
    greet("World")
}
EOF

# Vérifier les types
nudgec check hello.ndg

# Exécuter sur la VM native (0$, sans clé d'API)
nudgec run hello.ndg

# Lancer les tests de régression hors-ligne
nudgec test hello.ndg
```

### 3. Leçons interactives

```sh
# Leçons interactives dans votre terminal
nudgec learn

# Initialiser un projet avec des modèles prêts à l'emploi
nudgec init my-agent --template triage
cd my-agent && nudgec check my-agent.ndg && nudgec run my-agent.ndg
```

### 4. Compilation vers Python ou TypeScript (Optionnel)

```sh
# Cible Python
pip install nudge-runtime
nudgec build hello.ndg              # génère out/hello.py
python3 out/hello.py

# Cible TypeScript
nudgec build-ts hello.ndg           # génère out/hello.ts
```

## Fonctionnalités principales

| Fonctionnalité | Description |
|:---|:---|
| **Appels LLM Typés** | Le schéma de sortie est un type, réparations automatiques si invalide |
| **Contrôle & Données** | `if/else`, boucles `for`, enregistrements `{k: v}`, indexation, `T?`, `T \| U` |
| **Effets Typés** | 10 effets connus suivis et vérifiés statiquement à la compilation |
| **Espace de travail sécurisé** | Système de fichiers cloisonné en bac à sable + stockage clé-valeur typé |
| **Interface Locale (UI)** | Vues typées, fenêtres, tests headless et fenêtre desktop native |
| **Tâches & Concurrence** | Spawn/join, annulation coopérative, `par race`, libération de budget |
| **Authentification Sécurisée** | Flux RFC 8628 OAuth Device Flow fail-closed natif |
| **Décisions Typées** | Classification groupée, scoring par rubriques (`/v1/systemone`) |
| **Utilisation d'ordinateur** | Observation d'écran, actions typées, liste blanche d'applications |
| **Moteur Bytecode VM** | VM pile native (~70x plus rapide en calcul pur, sûreté récursive) |
| **Agents Autonomes** | Exécutable unique sans dépendance Cargo (~2.09Mo live, ~832Ko replay) |
| **Cible WebAssembly** | Replay hors-ligne dans WASI (~526Ko) et pont d'outils live |
| **Traces Déterministes** | Format ouvert NTF v1, rejouabilité à 0$, validation CI |
| **Confidentialité & Masquage** | Masquage automatique des secrets et données personnelles |

## Variables d'environnement

| Variable | Portée | Description | Valeur par défaut |
|:---|:---|:---|:---|
| `NUDGE_PROVIDER` | les deux | Fournisseur LLM actif (`fake`, `openai`, `anthropic`, `gemini`, etc.) | `fake` |
| `NUDGE_VM` | natif | Moteur Bytecode VM (`1` = VM par défaut, `0` = interpréteur AST) | `1` |
| `NUDGE_BUDGET` | les deux | Plafond budgétaire global en USD | Aucun |
| `NUDGE_TRACE` | les deux | Chemin de destination du fichier de trace NTF JSONL | `trace.jsonl` |
| `NUDGE_REPLAY` | les deux | Chemin de trace pour le replay déterministe hors-ligne | Aucun |
| `NUDGE_TRACE_REDACT` | natif | Masquage automatique des secrets et données sensibles (`1` ou `0`) | `1` (Actif) |
| `NUDGE_WORKSPACE_DIR` | natif | Répertoire racine de l'espace de travail sécurisé | Répertoire courant |
| `NUDGE_STORE_PATH` | natif | Fichier de stockage clé-valeur persistant | `<workspace>/.nudge/store.json` |

## Licence

Nudge est distribué sous licence **Apache-2.0**.
