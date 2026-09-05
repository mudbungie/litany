+++
title = "src/prompt/inbox/mod.rs rides one line under the 300 cap, so the next edit to it pays a split it did not cause"
created = 1788581075
updated = 1788581077
priority = 3
root_commit = "12899370c9ec7a5ed7f8e26d3d4fb914ea6c3310"
+++
299 lines after bl-b5b1, which added a parameter and a doc line and had to trim its own prose twice to stay lawful — and one of those trims already spent the split budget on the sibling test file (`tests/probe.rs` became `probe.rs` + `launcher.rs`). The cap is a wall, not a target: a file resting on it fires on whoever touches it next, at the moment they are finishing something else, when the cheapest way out is exactly the shave the rule forbids.

Split along a real seam rather than shaving. The file holds three axes that a reader already separates: the inbox paths and the deposit/drain vocabulary, the probe-and-launch decision (`probe_and_launch`, `Launcher`, `AdvanceLauncher`), and the CLI orchestration (`cli_message`, `cli_run`, `resolve_cli_sender`). The tests beneath it are already split on the second and third of those, which is evidence the seam is real and not invented for the line count.

Gates: tests 100 percent, docs, alignment.