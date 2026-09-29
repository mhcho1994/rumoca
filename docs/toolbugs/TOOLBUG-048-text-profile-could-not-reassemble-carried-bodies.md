# TOOLBUG-048 — the text profile could not reassemble a model with a function

**Status:** fixed (this change).
**Severity:** medium — `emit-text` / `assemble` broke for any model that
calls a function, and silently dropped call edges before that.

## What

Once function bodies were carried, `bitcode emit-text` printed a Modelica
body as `body modelica N statements not in text` and each function-value
node as `function-body-value`. `bitcode assemble` rejected its own output:

```
line 17: unknown function body `modelica`
```

That also broke the text module's stated contract ("printing a construct
this version cannot represent is an error, not an omission"). Separately,
the one-line `fn` form never wrote `RbcFunction::calls`, so a text round
trip had been losing the call graph since call edges were added, and it did
not carry the external ABI, value tables, folds or parameter spans either.

## Fix

A function carrying anything the one-line form omits is written as a
`function_record` holding the whole `RbcFunction` as quoted JSON, the way
`clock_record` already carries clocks; simple functions keep the one-line
form. The owner tables (`model_event_transaction`, `previous_value`,
`terminal_record`, `structured_root`, `delay_record`) are carried the same
way, so `emit-text` no longer refuses them. Function-value nodes get text
forms: `fnvalue ~f v def d`, `foldparam ~f fold k def d`,
`foldout ~f fold k def d`.

On six fixtures covering bodies with nested loops, transactions, delays,
`previous`, `terminal()`, structured roots and element-wise operators, the
reassembled artifact's JSON is identical to the original. Pinned by
`bitcode_function_import::the_text_profile_round_trips_a_carried_body_exactly`.
