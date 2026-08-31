+++
title = "alignment: the rule and the ruling read against the spec docs"
created = 1788146886
updated = 1788146886
parent = "bl-1408"
root_commit = "12899370c9ec7a5ed7f8e26d3d4fb914ea6c3310"
+++
Gate subtask for bl-1408. Check the delivery against `docs/ARCHITECTURE.md`, `docs/PRINCIPLES.md` and `docs/TAXONOMY.md` — in particular §2.1 terminology: the word "session" appears only inside the quoted term *agent-session URL*, the ported rule name `session-artifact`, and vendor key names, never as an interaction-span term of this project. The term is defined at its first use.