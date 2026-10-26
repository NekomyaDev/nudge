# 23/60 - Prompt syntax and multi-line blocks

`llm"""..."""` is a multi-line prompt literal. Its indentation and
newlines are preserved, so use blank lines and labelled sections to make the
request easy for a model to follow. A prompt ends only at the next `"""`.

Interpolation uses braces. To show literal braces in prose or JSON examples,
escape them as `\{` and `\}`. Keep instructions, input, and output contract in
separate sections rather than building one long ambiguous sentence.

Try:
  nudgec check prompt_syntax.ndg
  nudgec build prompt_syntax.ndg
