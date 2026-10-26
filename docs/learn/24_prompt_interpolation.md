# 24/60 - Prompt interpolation and formatting

Every `{name}` or dotted `{record.field}` reference in a prompt is checked
against the function's local variables. This catches misspellings before a
model sees the request. Put values next to clear labels so their role is
unambiguous.

Nudge currently interpolates values directly. Format values in ordinary Nudge
code before the call when you need a specific display form, then interpolate
the prepared variable.

Try:
  nudgec check prompt_interpolation.ndg
  nudgec build prompt_interpolation.ndg
