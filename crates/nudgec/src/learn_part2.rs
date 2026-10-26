//! Part 2 of the `nudgec learn` curriculum: LLM prompting and reliability.
//!
//! These examples intentionally use only syntax understood by the compiler
//! they ship with. The test at the bottom protects that promise.

use crate::learn::Lesson;

pub const LESSONS_PART2: &[Lesson] = &[
    Lesson {
        name: "prompt_syntax",
        title: "23/35 - Prompt syntax and multi-line blocks",
        body: r#"`llm"""..."""` is a multi-line prompt literal. Its indentation and
newlines are preserved, so use blank lines and labelled sections to make the
request easy for a model to follow. A prompt ends only at the next `"""`.

Interpolation uses braces. To show literal braces in prose or JSON examples,
escape them as `\{` and `\}`. Keep instructions, input, and output contract in
separate sections rather than building one long ambiguous sentence.

Try:
  nudgec check prompt_syntax.ndg
  nudgec build prompt_syntax.ndg"#,
        program: Some(
            r#"fn main(topic: string) -> string uses LLM {
    llm"""Write a concise explanation of this topic.

Topic:
{topic}

Return plain text. A literal JSON example can use \{ and \} safely.
"""
    with { model: "fake:default", budget: 0.01 USD }
}
"#,
        ),
    },
    Lesson {
        name: "prompt_interpolation",
        title: "24/35 - Prompt interpolation and formatting",
        body: r#"Every `{name}` or dotted `{record.field}` reference in a prompt is checked
against the function's local variables. This catches misspellings before a
model sees the request. Put values next to clear labels so their role is
unambiguous.

Nudge currently interpolates values directly. Format values in ordinary Nudge
code before the call when you need a specific display form, then interpolate
the prepared variable.

Try:
  nudgec check prompt_interpolation.ndg
  nudgec build prompt_interpolation.ndg"#,
        program: Some(
            r#"type Customer = { name: string, plan: string }

fn welcome(customer: Customer, balance: float) -> string uses LLM {
    llm"""Write a helpful billing reply.

Customer name: {customer.name}
Subscription plan: {customer.plan}
Current balance: {balance}

Use a warm, professional tone and state the next action clearly.
"""
    with { model: "fake:default", budget: 0.01 USD }
}
"#,
        ),
    },
    Lesson {
        name: "structured_schemas",
        title: "25/35 - Nested structured-output schemas",
        body: r#"A `schema:` makes an LLM call return a typed value instead of unstructured
text. Records may be nested and may contain lists of other records. Describe
every field in the prompt, including the allowed values of enum-like string
fields, so the model and runtime share the same contract.

Nudge's current schema syntax expresses enum-like choices as a string field
with an explicit finite set of values in the prompt. Use a discriminator field
inside a nested record when variants need different payloads.

Try:
  nudgec check structured_schemas.ndg
  nudgec build structured_schemas.ndg"#,
        program: Some(
            r#"type Evidence = { quote: string, source: string }
type Classification = { label: string, confidence: float }
type Analysis = { classification: Classification, evidence: [Evidence], summary: string }

fn analyze(text: string) -> Analysis uses LLM {
    llm"""Analyze the supplied text and return a structured analysis.

Text:
{text}

classification.label: exactly one of "positive", "neutral", or "negative"
classification.confidence: a number from 0 to 1
evidence: a list of records, each with quote and source
evidence.quote: a supporting excerpt
evidence.source: "input"
summary: one concise explanation
"""
    with { schema: Analysis, model: "fake:default", budget: 0.02 USD, retry: 2 with repair }
}
"#,
        ),
    },
    Lesson {
        name: "model_budget",
        title: "26/35 - Budgets, token caps, and hard limits",
        body: r#"Put a hard `budget` on every production LLM call. The value is a money
literal such as `0.05 USD`; it is the upper spending bound for that call.
`max_tokens` additionally limits response length, which controls latency and
keeps a verbose model from consuming the whole budget.

Budgets are contracts, not estimates. Set the cap from the largest acceptable
per-request cost, then monitor traces before raising it.

Try:
  nudgec check model_budget.ndg
  nudgec cost model_budget.ndg"#,
        program: Some(
            r#"fn summarize(document: string) -> string uses LLM {
    llm"""Summarize this document in at most five bullet points.

Document:
{document}

Return only the bullet points.
"""
    with {
        model: "fake:default",
        budget: 0.05 USD,
        max_tokens: 300
    }
}
"#,
        ),
    },
    Lesson {
        name: "model_routing",
        title: "27/35 - Selecting and routing models",
        body: r#"The `model` option names a provider and model with `provider:model`. This
makes routing explicit and keeps provider decisions close to the task:
`anthropic:sonnet-4.6`, `openai:gpt-4o`, and `fake:default` are typical
examples.

Use `fake:default` for deterministic offline development and tests. Switch to
a production model only when the call is ready to use real credentials and
spend real budget.

Try:
  nudgec check model_routing.ndg
  nudgec build model_routing.ndg"#,
        program: Some(
            r#"fn answer(question: string) -> string uses LLM {
    llm"""Answer the question in one accurate paragraph.

Question:
{question}
"""
    with { model: "anthropic:sonnet-4.6", budget: 0.03 USD }
}

fn offline_answer(question: string) -> string uses LLM {
    llm"""Answer this test question in one short sentence.

Question:
{question}
"""
    with { model: "fake:default", budget: 0.01 USD }
}
"#,
        ),
    },
    Lesson {
        name: "retry_repair",
        title: "28/35 - Retry and automatic JSON repair",
        body: r#"Structured responses can fail validation because a provider returns malformed
JSON or omits a required field. `retry: 3 with repair` gives the runtime up to
three attempts and asks it to repair invalid structured output before giving
up.

Use repair with schemas, but still make the prompt state every field and its
shape. Repair is a reliability net, not a substitute for a clear contract.

Try:
  nudgec check retry_repair.ndg
  nudgec lint retry_repair.ndg"#,
        program: Some(
            r#"type Todo = { title: string, priority: string, next_step: string }

fn make_todo(request: string) -> Todo uses LLM {
    llm"""Convert this request into one actionable todo item.

Request:
{request}

title: a short task name
priority: one of "low", "normal", or "high"
next_step: the first concrete action
"""
    with { schema: Todo, model: "fake:default", budget: 0.02 USD, retry: 3 with repair }
}
"#,
        ),
    },
    Lesson {
        name: "fallback_models",
        title: "29/35 - Fallback models for failures and timeouts",
        body: r#"A fallback model keeps a bounded task available when the primary provider
fails or times out. Declare the primary in `model` and the secondary in
`fallback`. Keep both prompts and schema-compatible expectations identical.

A fallback is not unlimited retrying: retain a budget, a response-size cap,
and a schema so the degraded path is safe and predictable too.

Try:
  nudgec check fallback_models.ndg
  nudgec build fallback_models.ndg"#,
        program: Some(
            r#"fn classify(message: string) -> string uses LLM {
    llm"""Classify this incoming message as billing, technical, account, or other.

Message:
{message}

Return exactly one category name.
"""
    with {
        model: "anthropic:sonnet-4.6",
        fallback: "fake:default",
        budget: 0.02 USD,
        max_tokens: 20
    }
}
"#,
        ),
    },
    Lesson {
        name: "few_shot",
        title: "30/35 - Few-shot prompting and context conditioning",
        body: r#"Few-shot prompts teach a format by showing representative input/output pairs
inside the prompt. Choose small, diverse examples that demonstrate the rule,
then clearly mark the new input. This is context conditioning, not training:
the examples only affect this request.

Keep examples relevant and compact because they consume context and cost.
Use a schema when the result must be machine-consumable.

Try:
  nudgec check few_shot.ndg
  nudgec build few_shot.ndg"#,
        program: Some(
            r#"fn label_review(review: string) -> string uses LLM {
    llm"""Label each review as positive, neutral, or negative.

Example input: "The delivery was fast and the product is excellent."
Example output: positive

Example input: "The package arrived yesterday."
Example output: neutral

New input:
{review}

Return exactly one label: positive, neutral, or negative.
"""
    with { model: "fake:default", budget: 0.01 USD, max_tokens: 10 }
}
"#,
        ),
    },
    Lesson {
        name: "prompt_clippy",
        title: "31/35 - Prompt Clippy reliability warnings",
        body: r#"`nudgec lint` runs Prompt Clippy before production does. W0001 flags a call
without a budget, W0002 flags a vague prompt, W0003 flags schema fields that
the prompt never mentions, W0004 flags a schema with no repair retry, and
W0005 flags an oversized decision option set.

Treat warnings as review prompts. The example is deliberately explicit so it
passes the important LLM-oriented checks.

Try:
  nudgec lint prompt_clippy.ndg
  nudgec check prompt_clippy.ndg"#,
        program: Some(
            r#"type Reply = { answer: string, confidence: float }

fn respond(question: string) -> Reply uses LLM {
    llm"""Answer the user's question with a factual, concise response.

Question:
{question}

answer: the direct answer in one or two sentences
confidence: a number from 0 to 1 indicating certainty
"""
    with { schema: Reply, model: "fake:default", budget: 0.02 USD, retry: 2 with repair }
}
"#,
        ),
    },
    Lesson {
        name: "temperature_params",
        title: "32/35 - Temperature, top_p, and seed parameters",
        body: r#"Sampling parameters control output variability. Lower `temperature` favors
repeatable, conservative wording. `top_p` limits sampling to the most likely
probability mass. A `seed` requests reproducibility from providers that
support deterministic sampling.

Use low-variance settings for extraction and classification. Use higher
creativity only when variation is the point, while retaining a schema and
budget for safety.

Try:
  nudgec check temperature_params.ndg
  nudgec build temperature_params.ndg"#,
        program: Some(
            r#"fn extract_fact(article: string) -> string uses LLM {
    llm"""Extract the single most important factual claim from this article.

Article:
{article}

Return one concise sentence and do not add speculation.
"""
    with {
        model: "fake:default",
        budget: 0.01 USD,
        temperature: 0.1,
        top_p: 0.9,
        seed: 42
    }
}
"#,
        ),
    },
    Lesson {
        name: "streaming",
        title: "33/35 - Streaming responses and token chunks",
        body: r#"Prefix an LLM binding with `stream let` when downstream code should consume
the response incrementally. The compiler records that the call is streamed;
the runtime can lower it to token-chunk processing rather than waiting for a
complete response.

Streaming improves perceived latency for long text, but it does not remove
budgeting or output validation. Keep the same explicit prompt contract.

Try:
  nudgec check streaming.ndg
  nudgec build streaming.ndg"#,
        program: Some(
            r#"fn draft(topic: string) -> string uses LLM {
    stream let response: string = llm"""Write a short, readable introduction to this topic.

Topic:
{topic}

Return three sentences.
"""
    with { model: "fake:default", budget: 0.02 USD, max_tokens: 180 }
    response
}
"#,
        ),
    },
    Lesson {
        name: "system_prompts",
        title: "34/35 - System prompts and role management",
        body: r#"Keep durable behavioral rules separate from per-request user data. The
`system` option carries role and safety instructions, while the `llm` block
contains the task-specific input. This separation makes reviews easier and
prevents user text from silently redefining the assistant's role.

State boundaries positively: describe what the model should do, what format it
must return, and which instructions are authoritative.

Try:
  nudgec check system_prompts.ndg
  nudgec build system_prompts.ndg"#,
        program: Some(
            r#"fn support_reply(customer_message: string) -> string uses LLM {
    llm"""Draft a reply to this customer message.

Customer message:
{customer_message}

Give a concise answer and a clear next step.
"""
    with {
        system: "You are a careful support assistant. Never invent account facts. Be polite and concise.",
        model: "fake:default",
        budget: 0.02 USD
    }
}
"#,
        ),
    },
    Lesson {
        name: "caching_cost",
        title: "35/35 - Prompt caching and cost optimization",
        body: r#"Large stable instructions can be cached by providers that support prompt
caching. For Anthropic-compatible calls, `cache_control: "ephemeral"` marks
reusable prompt context. Pair it with a stable system prompt and keep changing
user data in the request body.

Caching reduces repeated-input cost and latency, but it does not replace a
per-call budget. Measure the effect with traces and static cost reports.

Try:
  nudgec check caching_cost.ndg
  nudgec cost caching_cost.ndg"#,
        program: Some(
            r#"fn summarize_policy(policy_text: string, question: string) -> string uses LLM {
    llm"""Use the policy context to answer the question accurately.

Policy context:
{policy_text}

Question:
{question}

Return a concise answer and cite the relevant policy wording.
"""
    with {
        system: "You answer only from the supplied policy context and say when it is insufficient.",
        cache_control: "ephemeral",
        model: "anthropic:sonnet-4.6",
        budget: 0.05 USD,
        max_tokens: 240
    }
}
"#,
        ),
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn part2_lesson_programs_type_check_against_the_shipping_compiler() {
        assert_eq!(LESSONS_PART2.len(), 13);
        for lesson in LESSONS_PART2 {
            let Some(src) = lesson.program else {
                continue;
            };
            let tokens = crate::lexer::lex(src)
                .unwrap_or_else(|error| panic!("lesson '{}' does not lex: {error:?}", lesson.name));
            let items = crate::parser::parse(tokens).unwrap_or_else(|error| {
                panic!("lesson '{}' does not parse: {error:?}", lesson.name)
            });
            let errors = crate::check::check(&items);
            assert!(
                errors.is_empty(),
                "lesson '{}' does not type-check: {:?}",
                lesson.name,
                errors.iter().map(|error| &error.msg).collect::<Vec<_>>()
            );
        }
    }
}
