---
name: model-manager
description: "Setting up which model each agent runs on, together with the user: list the models they own and have connected, brainstorm a mapping to fit their budget and speed, test each candidate is alive before assigning it, then apply it per agent. Read when the user asks to choose, change, optimise or review the models their agents use, not for a one-off model switch."
---

# Setting up a model per agent

Every agent in enx runs on a model. By default it is the one model in use, or
the model its tier declares. A user who has connected several providers can do
better: a fast cheap model for mechanical work, a strong one for the agent
whose mistakes are expensive, a long-context one for the agent that reads a lot.
This skill helps the user decide that, and sets it, after checking each model
actually answers.

## When

Use this when the user wants help with the models their agents use:

- "set up models for my agents", "which model should each agent use"
- "make `fe` use a cheaper model", "optimise my model setup"
- "my reviewer is slow", "assign a model to the security agents"

Do not use it for a plain model switch for the conversation ("use gpt-5
now"): that is `/model`, one line, no skill.

## How it fits together

- A per-agent model wins over everything: `agent.models.<agent>`.
- Then the agent's tier model: `agent.tiers.{cheap,balanced,strong}`.
- Then the one model in use, `model.active`.

So you can set a model for one agent and leave the rest on their tier or the
shared model. A model that cannot run (no such provider, or no key) is skipped
at the turn, and the agent falls back to the model in use, so assigning a dead
model silently does nothing useful. That is why every candidate is tested
first.

The commands, run through the shell tool:

```
enx models list                 the connected providers and their models,
                                 the model in use, the per-tier and per-agent
                                 models
enx models list --plain         one provider/model per line, for picking from
enx models test provider/model  one tiny call; prints ok + latency + tokens,
                                 or fails with the provider's error
enx config set agent.models.<agent> provider/model    assign a model
enx config get agent.models.<agent>                   read one
enx config set agent.models.<agent> ""                clear it (back to tier)
```

`enx models test` is the gate: assign a model to an agent only after it has
answered.

## The steps

1. **See what the user has.** Run `enx models list`. It shows the connected
   providers and the models each lists, the model in use, the per-tier models,
   and whatever is already set per agent. If a provider shows no cached list,
   tell the user to open `/model` in enx once (or press Ctrl+R there) so enx
   fetches it; you cannot test a model you cannot name.

2. **Learn which agents matter and what the user wants.** Ask, in one `ask`
   with a short list of questions, only what the list does not answer:
   - which agents they actually use (the orchestrator or maestro that leads,
     `fe`/`be`/`db`/`mobile`/`systems`/`devops` that build, `reviewer` that
     checks, the security agents, `content-creator`);
   - what matters more for each, cost or quality or speed, and any budget;
   - whether any work needs a long context or vision.
   If they just say "you pick", propose a sensible mapping and say why.

3. **Propose a mapping.** A short table: agent, model, one reason. Keep it to
   the agents that matter; leave the rest on the shared model. A good shape:
   a strong model for the lead and the reviewer, a fast cheap model for the
   builders' mechanical passes, a long-context model for an agent that reads
   whole codebases. Check it with the user before touching anything.

4. **Test every candidate before assigning it.** For each distinct model in
   the mapping, run `enx models test provider/model`. Report what came back:
   alive and how fast, or the error. Never assign a model that failed; offer
   the user an alternative from the list instead. Testing spends a few tokens
   per model, so say so if the list is long, and test only the models you are
   about to assign.

5. **Apply the ones that passed.** For each, `enx config set agent.models.<agent> provider/model`.
   Use the agent's id (`fe`, `reviewer`, `security-recon`), not its display
   name. To put an agent back on its tier or the shared model, set the value
   to `""`.

6. **Confirm.** Run `enx models list` again and show the user the new per-agent
   block. Tell them a running enx picks up the change on its next turn; the
   footer and the agent roster then show each agent's model. If you changed the
   model for the agent leading the current conversation, note that the footer
   updates as soon as the change is saved.

## Keep it honest

- Test before you assign. "It should work" is not "it answered".
- Do not invent models. Only assign a `provider/model` that appears in
  `enx models list`, so the provider is connected and the id is real.
- Do not touch keys or providers here. If a provider the user wants is not
  connected, say so and point them to `/provider` in enx or `enx auth login`;
  connecting it is their step, not this skill's.
- Report a failed test as a failure, with the provider's own message, not as a
  model you quietly skipped.
