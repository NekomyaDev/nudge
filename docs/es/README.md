# Nudge: Lenguaje Tipado y Determinista para Agentes de IA

[English](../../README.md) | [Español](README.md) | [Deutsch](../de/quickstart.md) | [Français](../../README.fr-FR.md) | [中文](../../README.zh-CN.md) | [日本語](../../README.ja-JP.md) | [Türkçe](../../README.tr-TR.md)

---

## Integración con Model Context Protocol (MCP)

Nudge cuenta con soporte nativo de primera clase para el **Model Context Protocol (MCP)**, permitiendo a los agentes interactuar con herramientas externas de forma segura y declarativa.

### Declaración de Herramientas MCP

Puede registrar y enlazar servidores MCP directamente con la directiva `tool`:

```nudge
tool search_docs(query: string) -> [string] {
    impl: mcp("filesystem").tool("read_multiple_files")
}

tool run_database_query(sql: string) -> [string] {
    impl: mcp("postgres").tool("query")
}
```

### Ejecución Segura y Delimitada

El typechecker de Nudge exige declarar el efecto `Tool` cada vez que se invoque una herramienta MCP (`uses Tool`), impidiendo llamadas encubiertas o no autorizadas a la infraestructura host.

```nudge
fn investigate(topic: string) -> string uses Tool, LLM {
    let docs = search_docs(topic)
    llm"""Sintetiza la información recuperada: {docs}""" with {
        model: "anthropic:sonnet-4.6",
        budget: 0.03 USD,
    }
}
```

### Replay y Determinismo

Todas las entradas y salidas de las llamadas a herramientas MCP se registran en trazas NTF estándar (`kind: "tool.call"`). Durante las pruebas o re-ejecuciones (`nudgec test`), las respuestas de las herramientas se reproducen de manera determinista sin contactar con los servidores MCP externos.
