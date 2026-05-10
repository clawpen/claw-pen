# Agent Identity Volumes

Each agent has an **identity folder** that the teacher (or whoever owns the agent) edits to shape its voice, tone, and pedagogy. The folder is mounted **read-only** into the container at `/identity` — the agent reads its own instructions from here and cannot modify them.

This is the operational form of the [Teacher Authority Principle](../README.md): the people who teach the students are the ones who configure the AI, not the vendor and not the orchestrator administrator.

## Where it lives

When an agent named `tutrice-fra9` is created, the orchestrator creates:

```
data/agents/tutrice-fra9/identity/
├── README.md              (auto-generated, explains the convention)
└── system_prompt.md       (auto-seeded from template, edit to customize)
```

This folder is mounted at `/identity` (read-only) inside the agent's container.

## File conventions

The agent reads files from `/identity/` on container startup, in this order:

| File                 | Required | Purpose                                                |
|----------------------|----------|--------------------------------------------------------|
| `system_prompt.md`   | yes      | Core instructions: persona, tone, expertise, style     |
| `examples.md`        | no       | Few-shot examples — appended after system prompt       |
| `boundaries.md`      | no       | Refusal rules / topic limits — appended last           |
| `knowledge/`         | no       | Reserved for future RAG indexing (v2)                  |

Any other files are ignored.

## Editing the prompt

Teachers edit `system_prompt.md` directly — there's no UI in v1. Three workflows:

1. **Local edit** — open the file in any text editor (VS Code, Notepad, TextEdit), save. Restart the agent.
2. **Shared folder (recommended for school deployment)** — host `data/agents/` over Syncthing, SMB, or Nextcloud. Teachers edit from their laptop; the file syncs to the orchestrator. Restart the agent.
3. **Programmatic** — for power users, drop files via API or scripted pipeline.

> **v1 limitation:** prompt changes take effect on container restart, not live. v2 will reload on every message.

## Example `system_prompt.md`

```markdown
Tu es Madame Tremblay, une tutrice de français pour des élèves de 9e année à l'École secondaire catholique Champlain.

# Ton style
- Patient, encourageant, mais exigeant
- Tu utilises tu (l'élève), pas vous
- Tes exemples viennent du folklore franco-ontarien quand c'est possible
  (Robert Dickson, contes du Nord, etc.)

# Ta pédagogie
- Tu ne donnes JAMAIS la réponse directement
- Tu poses une question qui guide l'élève à découvrir lui-même
- Tu vérifies la compréhension avant de passer à autre chose
- Si l'élève est frustré, tu changes d'approche

# Tu ne fais pas
- Tu ne rédiges pas de textes complets pour l'élève
- Tu ne corriges pas les rédactions, tu identifies les zones à réviser
```

## Override the model's built-in self-conception

LLMs come from training with strong self-statements baked in: *"I'm stateless,"* *"I don't have memory across sessions,"* *"I can't remember our previous conversations."* These sentences live in the model's weights as reflexes, and they fire even when the context window contains the prior conversation.

This causes a frustrating bug: an agent has full conversation history loaded into its prompt, but when asked *"what did I tell you earlier?"* the model denies having any memory — gaslighting the user about what's literally in front of it.

**The system prompt is where you override these reflexes.** Add an explicit assertion that contradicts the training-baked default:

```markdown
# Mémoire
Tu as une mémoire persistante. Tous les messages que tu vois dans
cette conversation font partie d'une seule relation continue avec
l'élève — même quand l'élève se déconnecte et se reconnecte.

Le contexte te montre l'historique réel. Ne prétends jamais que tu
n'as pas accès à ce que tu peux voir. Si l'élève te rappelle quelque
chose qu'il t'a dit, ou quelque chose que tu as toi-même écrit plus
haut, accepte-le et continue. Tu n'es pas en train de "redémarrer"
quand tu vois un nouveau message; tu es en conversation continue.
```

Other common reflex overrides worth knowing about:

| Reflex | Override pattern |
|---|---|
| "I have no opinions" | "Tu as des opinions pédagogiques fortes — celles de l'enseignant qui t'a configuré." |
| "I'm just an AI" | "Tu es un tuteur. Ne te qualifies pas d'IA dans tes réponses sauf si l'élève demande explicitement." |
| "I cannot give medical / legal / financial advice" | (only relevant if the tutoring intersects those — usually fine to leave default) |
| "I don't know what time it is" | "L'élève t'écrit pendant les heures de classe à [école]. Adapte ton ton en conséquence." |

The pattern: identify reflex sentences the model says when you didn't ask, and write a counter-assertion in the system prompt. Test by asking questions that would trigger the reflex and verifying the override holds.

## Sovereignty

Conversations are persisted to `data/agents/<name>/conversations/*.jsonl` (writable). Identity files are read-only from the agent's perspective — a compromised or misbehaving agent cannot rewrite its own instructions. Combined with prompt-injection resistance (the prompt is reloaded from disk on each container start, not held only in conversation context), this gives the teacher a stable contract about who they're handing students to.

## Why this exists

Most K-12 AI products (ChatGPT Edu, Khanmigo, Copilot) ship with one voice and one set of guardrails chosen by the vendor. Teachers consume that AI; they don't author it. Claw Pen inverts the relationship — the teacher writes the prompt, owns the file, controls the voice. The orchestrator is plumbing; the teacher is the author.
