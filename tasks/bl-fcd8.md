+++
title = "prime seeds an empty workflows/ dir: seed workflows/basic-agentic-loop.yaml from the embedded template so the named default exists as an authoring source"
created = 1788236069
updated = 1788236069
root_commit = "12899370c9ec7a5ed7f8e26d3d4fb914ea6c3310"
+++
ARCH §2.2 names <config-root>/workflows/ as the template pool config commits are authored from, but prime seeds it empty — the basic agentic loop (bl-f928, = template/workflow.yaml) has no named file there. Seed it seed-if-absent from the SAME embedded asset template/workflow.yaml (one home, two seeding paths). Touches src/install.rs and src/install/tests — the install/tool-seeding area has concurrent work in flight; coordinate before claiming.