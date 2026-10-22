+++
title = "role-generic verdict events: <role>_<token> fires when the workflow binds it and the child's leading line says it; else <role>_return; the gate generalizes over them"
created = 1792631678
updated = 1792632653
claimant = "Satellite"
parent = "bl-06ae"
priority = 2
root_commit = "12899370c9ec7a5ed7f8e26d3d4fb914ea6c3310"

[[blockers]]
id = "bl-041b"
on = "claim"
+++
Implements DESIGN_PROCEDURES §5. The event set opens along one axis: a return event per role and a verdict event per (role, token) the workflow binds. Guards: role from the dispatch commit subject; verdict read only in the dispatcher's inbox; a token nobody bound is no event. verifier_approve/verifier_reject keep parsing — reject is the verifier's unmatched return.