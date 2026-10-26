# 43/60 - escalate low-confidence decisions to a human

A human-in-the-loop policy makes uncertainty explicit. Let high-confidence
cases proceed automatically, but return an escalation state for the remainder.
The escalation can become a queue entry, UI task, or an external review tool.

The important invariant is that the low-confidence path remains safe and
observable, rather than silently guessing.

Try:
  nudgec check human_in_the_loop.ndg
  nudgec build human_in_the_loop.ndg
