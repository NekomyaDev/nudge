# Code Reviewer

An AI agent that reviews code and provides actionable feedback.

## Features

- Code quality analysis
- Security vulnerability detection
- Performance suggestions
- Best practice recommendations
- Budget-controlled reviews

## Code

```nudge
type Issue = { severity: string, line: int, message: string, suggestion: string }
type Review = { summary: string, issues: [Issue], score: float @range(0, 10) }

fn review_code(code: string, language: string) -> Review uses LLM {
    llm"""Review this {language} code for quality, security, and performance.

    Code:
    ```{language}
    {code}
    ```

    Identify issues with severity (critical/high/medium/low), line numbers, and suggestions.
    Give an overall quality score from 0-10."""
    with { schema: Review, model: "anthropic:sonnet-4.6", budget: 0.03 USD, retry: 2 with repair }
}

fn main() -> Review uses LLM {
    let code = "def process(data):\n    exec(data)\n    return eval(data)"
    review_code(code, "python")
}

test "review identifies security issues" {
    let t = replay("traces/review.jsonl")
    assert len(t.output.issues) >= 1
    assert t.output.score < 5.0
    assert t.cost_usd < 0.05
}
```

## Run

```sh
nudgec check code-reviewer.ndg
nudgec build code-reviewer.ndg
python3 out/code-reviewer.py
```

## How it works

1. Takes code snippet and language
2. Analyzes for issues (security, performance, quality)
3. Returns structured review with line numbers
4. Budget enforced per-call ($0.03 USD)
