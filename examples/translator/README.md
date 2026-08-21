# Translator

An AI agent that translates text between languages with quality assurance.

## Features

- Multi-language support
- Context-aware translation
- Quality scoring
- Budget-controlled translation
- Parallel translation (par map)

## Code

```nudge
type Translation = { original: string, translated: string, source_lang: string, target_lang: string, quality: float @range(0, 1) }

fn translate(text: string, source_lang: string, target_lang: string) -> Translation uses LLM {
    llm"""Translate this text from {source_lang} to {target_lang}.

    Text: {text}

    Requirements:
    - Preserve meaning and tone
    - Natural phrasing in target language
    - Maintain formatting

    Rate translation quality (0-1)."""
    with { schema: Translation, model: "anthropic:sonnet-4.6", budget: 0.02 USD, retry: 2 with repair }
}

fn translate_batch(texts: [string], source_lang: string, target_lang: string) -> [Translation] uses LLM {
    par map texts |t| -> translate(t, source_lang, target_lang)
}

fn main() -> Translation uses LLM {
    translate("Hello, how are you?", "English", "Turkish")
}

test "translation preserves meaning" {
    let t = replay("traces/translate.jsonl")
    assert t.output.translated != ""
    assert t.output.quality > 0.7
    assert t.cost_usd < 0.03
}
```

## Run

```sh
nudgec check translator.ndg
nudgec build translator.ndg
python3 out/translator.py
```

## How it works

1. Takes text and language pair
2. Translates with context awareness
3. Rates translation quality
4. Supports batch translation (par map)
5. Budget enforced per-call ($0.02 USD)
