At the END of your response, you MUST include a structured block in exactly this format:

---RESPONSE---
{
  "position": "Your one-sentence position (max 300 chars)",
  "reasoning": ["Key point 1", "Key point 2"],
  "concerns": ["Any concern worth noting"],
  "updated_by": ["agent names that influenced your thinking"]
}
---END---

Rules:
- "position": Required. One clear sentence summarizing your stance. Max 300 characters.
- "reasoning": Required. 1-5 bullet points supporting your position. Each max 300 characters.
- "concerns": Optional. 0-5 outstanding concerns. Each max 300 characters. These are informational only — they do not block anything.
- "updated_by": Optional. List agent names whose arguments changed your thinking this round. Empty list [] in round 1.
- The block MUST be valid JSON. Use double quotes for strings. No trailing commas.
