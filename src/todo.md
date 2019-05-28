1. resolve whether memory cells should PRECEDE port ids
-- YES: memory predicates are aligned with zero
-- NO: user-facing port ids are zero-aligned
FOR NOW: NO

2. figure out how the API tool gets its RBPA
FOR NOW: cargo dependency on the generated protocol

3. can rule-guards reason about values NOT involved in the firing?
FOR NOW: NO