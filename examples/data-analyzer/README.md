# Data Analyzer

An AI agent that analyzes data and produces insights.

## Features

- Data pattern recognition
- Statistical insights
- Trend analysis
- Budget-controlled analysis
- Structured output

## Code

```nudge
type Insight = { category: string, finding: string, impact: string, confidence: float @range(0, 1) }
type Analysis = { dataset: string, insights: [Insight], summary: string, recommendations: [string] }

fn analyze_data(data: string, context: string) -> Analysis uses LLM {
    llm"""Analyze this data and provide insights.

    Data:
    {data}

    Context: {context}

    Identify:
    - Key patterns and trends
    - Statistical insights
    - Business impact
    - Actionable recommendations

    Rate confidence for each insight (0-1)."""
    with { schema: Analysis, model: "anthropic:sonnet-4.6", budget: 0.04 USD, retry: 2 with repair }
}

fn main() -> Analysis uses LLM {
    let data = "Q1: $100k, Q2: $150k, Q3: $120k, Q4: $200k"
    let context = "Annual revenue analysis for SaaS startup"
    analyze_data(data, context)
}

test "analysis provides actionable insights" {
    let t = replay("traces/analysis.jsonl")
    assert len(t.output.insights) >= 2
    assert len(t.output.recommendations) >= 1
    assert t.cost_usd < 0.06
}
```

## Run

```sh
nudgec check data-analyzer.ndg
nudgec build data-analyzer.ndg
python3 out/data-analyzer.py
```

## How it works

1. Takes data and context
2. Identifies patterns and trends
3. Provides statistical insights
4. Returns actionable recommendations
5. Budget enforced per-call ($0.04 USD)
