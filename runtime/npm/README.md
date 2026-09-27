# @nekomyadev/nudge-runtime

TypeScript runtime for the [Nudge language](https://github.com/NekomyaDev/nudge) —
typed LLM and decision-model calls with schema validation, `$0` replay and
open NTF traces. Zero dependencies.

```sh
npm i @nekomyadev/nudge-runtime
```

```ts
import * as rt from "@nekomyadev/nudge-runtime";
const out = rt.llmCall({ prompt: "Summarize: ...", model: "fake", budget: rt.USD("0.01") });
```

The TS runtime covers the fake provider, schema validation, replay, property
tests (`forAll`), decisions (`decide`) and NTF traces. Live LLM providers and
the OTel/OTLP export run on the Python runtime (`pip install nudge-runtime`);
compile with `nudgec build` for those.
